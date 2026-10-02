# Spec: reliable full-package activation before delta optimization

Status: approved
Linear: KEL-53 · Owner: GYLDLAB · Updated: 2026-09-26
Approval: original corrected contract: Linear comment `b343d835-1528-461f-bda4-0fa5e238b5af` · approved corrected content head `a340acf0b5cfcbfab9111f938cd3ac2788219ccb` · decision SHA-256 `972b82947189b5d89c7c78d11547f0c0ef890a60bf36af1bdfb68d448fed2ed9`; KEL-263 producer-host and policy-owner amendment: delegated approval comment `df61a6f6-3215-44a8-8780-7ae242fc74ab` · decision SHA-256 `871a803ba4c04087209ebb7a19751a15382dcae42cd8764d2cd7090d3cc9ba83` · approved content head `3d5d51e31237c365847a1e9b866880ba06429e2e`; complete crate dependency graph: supplemental delegated decision comment `212a2039-f729-4708-a983-9a348d299bd5` · decision SHA-256 `939cc2100ba25b172088371c1311e7555156c0f8e9430dd8ad38998caa5d0fc8` · approved content head `3d5d51e31237c365847a1e9b866880ba06429e2e`.

{"schema":"keld.kel53-approval/v1","decision":"approved","approved_content_head":"a340acf0b5cfcbfab9111f938cd3ac2788219ccb","approver_id":"49ccfebb-c3fb-40a3-abb2-a3bf92e83cb1","linear_comment_id":"b343d835-1528-461f-bda4-0fa5e238b5af","source":"active-maintainer-session-2026-09-20"}
{"schema":"keld.kel53-approval/v1","decision":"approved","approved_content_head":"3d5d51e31237c365847a1e9b866880ba06429e2e","approver_id":"49ccfebb-c3fb-40a3-abb2-a3bf92e83cb1","linear_comment_id":"df61a6f6-3215-44a8-8780-7ae242fc74ab","source":"delegated-maintainer-session-2026-09-26"}
{"schema":"keld.kel53-approval/v1","decision":"approved","approved_content_head":"3d5d51e31237c365847a1e9b866880ba06429e2e","approver_id":"49ccfebb-c3fb-40a3-abb2-a3bf92e83cb1","linear_comment_id":"212a2039-f729-4708-a983-9a348d299bd5","source":"delegated-maintainer-session-2026-09-26"}

Windows multi-mode product decision and T1 synchronization authorization: owner decision
recorded in Linear comment `e5572b4a-9643-4847-8ed2-c419e2198fc6`, bound to the
pre-receipt activation-spec SHA-256 `4213CE3E9BED2EA380622A162763702EC321DC50297E2DB275844A983B73EAFF`
and Architecture 06 SHA-256 `F14586C97AA475561A15DDFDB988E839E3FB6594DE3407C65669AA5EF3F8D40E`.
The no-repeat-UAC product mode is approved; its specific privileged mechanism is not.

KEL-265 T3b amendment: delegated approval comment
`559df4c6-b8b1-4a47-ae81-2311e2743eb6`, approved content head
`bb3d863e20d495349aa64807bafa598ffa4f31b0`, file SHA-256
`f9ab8891c061150b8f3fe90884b181fbd6ebd11ff49b605b8c6570a5904f800c`.
The maintainer delegated this bounded decision in the active session; native
acceptance and independent implementation review remain required.

KEL-266 T4a amendment: delegated approval comment
`a059df2c-e9fe-4eec-9582-92b6aa5cedb0`, approved content head
`a5fc30808e61f8c1c0707759c40190c4efbec7f8`, file SHA-256
`649b2f97d72a06ebd61a2fca23721a1ba75e28f9477a7252c6961ce5071e0ba5`.
The independent design approval does not replace native qualification or final-diff
security, unsafe and public-contract review.

KEL-270 T4b amendment: owner approval comment
`a0329498-a115-4bcb-a277-0f32a42d7a86` (all four points), approved content head
`caf2c82b52b2c4a95d5fd0ce2003e611267439e2`, file SHA-256
`4355824b8514acd51713ff920b5b4143759a9f3563209eab3df4a51707ddefed`. The owner approved it
in the active maintainer session on 2026-10-02. Four bounded changes to the common
transaction, each narrowing a liveness or replay gap; none grants authority beyond the
existing exclusive writer lease, and item 4 adds one public repair entry point:
1. Retire each unreferenced version under the journal before journal removal, and
   remove the journal by write-through rename, instead of removing the journal first
   and cleaning retention afterwards.
2. Resume an unlaunched `PublishPending` attempt under the exclusive writer lease alone.
3. Durably re-mint the health and lifecycle channel identities whenever an attempt is
   resumed.
4. Add an explicit repair entry that, under the exclusive writer lease and only when no
   journal exists and every record validates, retires complete versions that no record
   references; the ordinary loader still halts on them.
Rationale and native evidence are in KEL-270 comments `b571afdb` and `f6b1e538` and the
T4b pull request.

KEL-266 AC4–6 completion: delegated approval comment
`bfeb14d0-e906-476f-970a-7fd837bc7f2f`, approved content head
`a7d54066704f08cb170435ad72877afdea93f6d1`, file SHA-256
`d43431e186fa20f1ca3d2281dfab06ef9fb8227be5044face730c3390b6896fb`.
This supplement closes coherent admission, higher-release staging and committed-state
LPAC evidence within the same issue; it does not claim activation or installed boot.

Windows direct-install mode amendment: exact content was approved in the user's
approval of Linear PR #290 head `b78c89b061049647421dda69034c9f35079d9441`, via Linear
comment `e0b276f9-5ecc-42a9-ac8f-8a5e05f44245`. The approved pre-receipt KEL-53 file
SHA-256 is `6b4fda6e8bcbb388886d8254d5bc8558cd706533e6c28a061c478dc1acc42442`. This
amendment supersedes any earlier implication that the machine-protected baseline is the
only direct-install mode or that one helper mechanism is selected. Its implementation
and native acceptance gates remain open; in particular, this approval does not select
or authorize the machine-seamless authority.
Post-approval consistency correction: the Machine-UAC summary now matches the detailed
adapter contract below: the request carries only a bounded, non-authoritative source
locator, and the elevated owner opens, pins and verifies the source handles itself.
This resolves contradictory summary wording without changing an install mode or
authority boundary.

## 1. Goal & non-goals

Keld's direct updater must prove one safe signed full-package
install/activation/recovery path. The first admitted package cell is Windows x64
direct distribution whose complete file tree fits the existing v0 canonical archive.
Its default installation mode is per-user with seamless no-UAC updates. An explicitly
selected Program Files installation supports both per-update explicit-UAC activation
and opt-in seamless machine-wide activation, with the latter's privileged mechanism
gated on separate proof. Package/deployment-managed installs defer mutation to their
owner. One updater verifies a signed release, stages exact bytes, journals and publishes
one attempt-bound candidate, confirms its exact health receipt, then commits or recovers
to the retained last-known-good package. Delta patches remain a later optional
transport optimization.

Non-goals:

- no delta algorithm or dependency in the first slice;
- no updater for package-manager/store-owned installs;
- no macOS/Linux package claim before KEL-137 supplies the executable-mode/link
  representation those cells require;
- no data-migrating release, migration hook, or claim that binary rollback rolls data
  back in the first slice;
- no arbitrary relaunch helper, shell command, self-update plugin, or role-writable
  update state;
- no inference of install mode from Program Files, LocalAppData, executable path,
  registry location, environment or writable configuration;
- no selected no-UAC machine coordinator until its named authentication, replay,
  writer, lifecycle, health and recovery proofs pass;
- no TUF-style rotating-root design beyond the existing v0 single-key limitation;
- no implementation before this approved corrected specification lands.

### Windows direct-install modes (approved contract; implementation gates remain)

One KEL-53 transaction owns signed package verification, anti-downgrade policy,
staging, the activation journal, installation-wide exclusive ownership, exact candidate
launch, health confirmation, commit/rollback and crash recovery. Install mode changes
only how that transaction obtains its activation write lease; it MUST NOT create three
updater state machines or move journal/pointer policy into the authority adapter.

| Mode | Install and update authority | Required boundary |
|---|---|---|
| `PerUserDirect` (default) | Install beneath the installing user's application location. Verify, stage and activate under that user's ordinary authority with no UAC. | The same-user owner can mutate its own files; Keld does not claim protection from arbitrary native malware under that account. Supervised app roles remain unable to mutate updater state where the admitted profile supports distinct OS principals. |
| `MachineUacDirect` | Install beneath a protected machine root. Download and verify into a separate user-owned staging area; when protected activation is required, request UAC and use the signed/elevated updater for the exact attempt. | The application host and Bun roles remain non-elevated/non-SYSTEM. Before elevation, bind the request to the initiating user's SID, logon-session identity and caller process. It may carry only KEL-53's bounded source lookup locator beneath that authenticated user's owner-private staging root; the locator conveys no authority and cannot select an install root or write destination. The elevated component's Authenticode signer and exact image digest must match the protected helper identity. It resolves the locator through the Windows path owner, opens and pins the exact read-only source handles, denies concurrent write/delete where the platform permits, and independently verifies the signed manifest/artifact from those handles. It then creates the protected journal/attempt, copies verified bytes into a protected sibling stage and verifies/read-backs the copy before publication. It accepts no mutation authority from argv, environment, cwd, or arbitrary caller paths and mutates only this installation's package/update roots through common KEL-53 code. Candidate launch must use the exact initiating user's ordinary token and logon session, even if UAC used alternate administrator credentials; if that token cannot be securely reused, it refuses launch and preserves journal-bound recovery. Forged, stale, replayed, wrong-host, cross-install, replaced-source or wrong-session requests refuse before protected mutation. If the elevated owner dies, recovery is journal-bound and fail-closed; retry may require another explicit UAC grant. |
| `MachineSeamlessDirect` | Install beneath a protected machine root; later activation obtains a narrow privileged write lease without repeated UAC. | Opt-in only. The mechanism is not selected by this requirement. It remains unavailable until the KEL-270 proof gate below passes and an exact-reviewed Windows-native authority is selected. Host/Bun remain ordinary-user processes. |
| `ManagedOwner` | MSIX, App Installer, Store, enterprise deployment and package-manager owners perform their own update. | Keld MUST refuse direct activation and MUST NOT register a competing writer or selector. |

For `MachineUacDirect`, `keld-update::WindowsUacAuthorityAdapter` owns capture and
transfer of the initiating user's actual primary-token and process capabilities. The
ordinary updater opens the primary token from its retained initiating-process handle;
the elevated owner validates that the transferred token is the token of that exact
process object, not a separately supplied token with matching strings. It rereads and
validates the token's user SID, logon-session
`AuthenticationId`, session id, elevation type and integrity level, plus the initiating
process identity/image, against the one-shot request. Identity strings alone MUST NOT
be used to recreate a token. Retain the exact token/process handles until the candidate
host is launched and its process handle is retained; retain the candidate process and
health witnesses through transaction commit or rollback. If the exact capability cannot
be transferred or its provenance cannot be validated—including when UAC used alternate
administrator credentials—refuse before protected mutation. If the token capability is
lost after mutation, roll back under the same writer lease or preserve the journal and
require explicit recovery. The one-shot authenticated request may name the staged source
only as a lookup locator beneath the authenticated user's owner-private staging root;
that locator conveys no write authority. While impersonating the authenticated client,
the adapter uses the existing Windows path owner to reject reparse/substitution and open
the source files read-only, retaining the exact file objects and denying concurrent
write/delete where the platform permits. It reverts before privileged operations,
verifies the signed manifest and archive from those same retained handles, copies into a
protected sibling stage, and reads back the exact bytes before KEL-53's normal
publication order. Any source race or inability to pin stable bytes refuses before
publication. It never reopens an untrusted source by caller-provided path after pinning.

**UAC launch qualification target, not yet proven:** evaluate an authenticated local
IPC request from the exact initiating process; impersonate the last authenticated
client message only at `SecurityImpersonation`; open and duplicate that thread token
into a primary token with `DuplicateTokenEx(TokenPrimary)`; always check impersonation
success and revert before protected updater operations. The candidate launch target is
`CreateProcessWithTokenW` using that retained primary token, only when the elevated
helper has the required `SeImpersonatePrivilege` and runs in the same interactive
session as the captured initiating user. Verify the resulting host token, session,
integrity/elevation, image, profile/environment and desktop before resources. A session
mismatch, missing privilege, identification-only token, failed reversion, unavailable
desktop/profile or failed token proof refuses before publication; if detected after
publication, roll back or preserve the journal under the same writer lease. Do not
substitute the elevated administrator token. `CreateProcessAsUser` is not an automatic
fallback: its token-session behavior and additional caller privileges need separate
exact proof. A pre-existing ordinary-user launcher MAY be reused only if its installed
lifecycle and authenticated handoff independently satisfy the same contract. This
qualification does not select or constrain the separate machine-seamless mechanism.

Mode is authenticated installation provenance, never inferred from Program Files paths,
ACL observations, executable location, command line, environment, or caller flags. The
same candidate bytes, floor, journal transitions, health predicate and rollback rules
apply to each direct mode. Only the lease-acquisition adapter and its native evidence
matrix vary.

`MachineSeamlessDirect` keeps KEL-270's approved bounded proof slice as a prerequisite:
exact attempt Job handle, bounded keeper, one-shot authenticated result transfer,
writer-lease/lifecycle coupling, and fail-closed recovery. No production pointer,
floor, activation, commit or rollback writes connect to that keeper slice until it
passes coordinator-death, keeper-death, wrong-Job/host, replay/stale-attempt, competing
writer, process-family-zero and ambiguous-reboot controls. A Windows Service, Scheduled
Task, installer helper or other mechanism is not preselected. If all-owners-lost or
reboot state cannot be proven, recovery preserves journal/pointers and halts.

The amendment expands acceptance coverage for AC1/6/7/8/10/12/14 and T4b/T5 into
independent per-user, machine-UAC, machine-seamless and managed-owner cells. The UAC
cell additionally tests request-to-user/session binding, exact signed helper identity,
initiating-token handle provenance and lifetime, TokenUser/AuthenticationId/session/
elevation/integrity validation, association to the initiating process, alternate UAC
credentials, unavailable/substituted token handles, one-shot source locator and
elevated-owner-opened read-only source handles,
source-file substitution/reparse/write races, independent signature/artifact
revalidation, replay and cross-install refusal, protected copy/readback, mutation
confinement, exact initiating-user candidate token, wrong-session refusal, process-family lifecycle,
helper/coordinator death at every persisted boundary, and journal-bound recovery with
renewed consent where required. A positive control proves the exact authorized operation
can complete. Per-user tests assert same-user no-UAC activation and its stated threat
exclusion. Machine-UAC
tests assert explicit elevation only at protected activation and prove the application
never runs elevated. Machine-seamless tests remain refusal-only until KEL-270's proof
gate and selected mechanism pass. Managed-owner controls prove no direct writer. Each
cell runs the same durable state-machine trace and crash-cut matrix; a pass in one
authority cell does not close another.

## 2. Spec refs

- `docs/architecture/06-runtime-and-tooling.md` §4/§4a owns the v0 feed,
  manifest, canonical archive, verification order, local trust floor and activation
  model.
- `docs/architecture/03-security.md` owns update trust, protected state and the
  narrow Windows relaunch-helper boundary.
- `keld-pack` owns the Windows v0 package producer and exact no-migration policy
  path/bytes, and depends on `keld-guard` for the single Windows package-path
  validator. `keld-update` depends on both `keld-guard` and `keld-pack` so it can consume
  that policy owner; packaging never depends on the updater. The first Windows x64
  producer runs on a Windows host to use the guard-owned Windows namespace contract.
  Other hosts refuse before output. Cross-host assembly remains the target; this
  support cell expands only after independent evidence proves Windows-name equivalence.
- KEL-137 owns a future canonical package representation with executable modes and
  bundle links. It precedes every macOS/Linux package cell and any Windows package that
  cannot fit the current regular-file/directory-only v0 archive.
- KEL-53 owns activation, health binding, channel-owner refusal, helper confinement,
  fault injection and binary-versus-data honesty.
- KEL-130 owns the shared Windows lexical-component classifier. Before T3 its owner
  must be amended to include the complete package-required forbidden/control and 8.3
  policy; package validation consumes it and must not copy a second list.
- KEL-90/KEL-129 own measurements and budgets.
- PR #30 landed the current signed-manifest/feed wire contract.
- Microsoft documents token-based process launch in [`CreateProcessAsUser`](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessasusera)
  and [`Processes in the Client Security Context`](https://learn.microsoft.com/en-us/windows/win32/secauthz/processes-in-the-client-security-context).
  These platform contracts do not by themselves prove Keld's initiating-user token
  handoff or process lifecycle.
- The Windows UAC qualification candidate also depends on Microsoft's contracts for
  [`ImpersonateNamedPipeClient`](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-impersonatenamedpipeclient),
  [`DuplicateTokenEx`](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-duplicatetokenex),
  and [`CreateProcessWithTokenW`](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createprocesswithtokenw).
  These define token type, access-right and session constraints; Keld still requires
  exact Windows proof for request binding and lifecycle.

This approved corrected contract keeps the v0 JSON fields but intentionally revises their validation,
client-selection and canonical-package semantics before any updater implementation
exists. Approval of the corrected exact content head recorded above re-approves the v0 public
contract; a post-implementation
semantic change requires a new schema/version and wire review. The contract reuses the
existing strict semantic-version floor. There is no release-sequence, expiry, or
security-epoch field to validate. Adding one also requires a new manifest version.

Current durability inputs are Microsoft `FlushFileBuffers` and
`MoveFileExW(MOVEFILE_WRITE_THROUGH)`, POSIX synchronized I/O and directory
cache requirements, and Apple's `F_FULLFSYNC`. They define adapter order;
implementation still needs real-OS crash-cut evidence for each admitted filesystem.
Windows lease sharing and lock lifetime follow the documented [`CreateFileW` share
compatibility and handle-close rules](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew);
record replacement uses only the documented same-volume [`MoveFileExW` replace-existing/
write-through flags](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw). Explicit UAC owner
assignment follows Microsoft's [`TOKEN_OWNER` valid-owner-group rule](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_token_owner)
and [owner rights](https://learn.microsoft.com/en-us/windows/win32/secauthz/owner-of-a-new-object); the candidate still runs in the initiating user's ordinary session under
[UAC](https://learn.microsoft.com/en-us/windows/security/application-security/application-control/user-account-control/how-it-works).

## 3. Acceptance criteria (binary, each becomes a test)

1. Given a direct-distribution installation, startup consumes installer-created
   `keld.install-provenance/v2` binding the exact app id, channel, target, install root,
   update root, installed baseline artifact, compiled-in signing-key identity, explicit
   install mode and mode-specific OS protection profile. Mode-less v1 provenance is
   unsupported until an explicit trusted migration records one mode; location never
   supplies that choice. Mutation checks are evaluated against the
   selected mode's admitted writer authority: provenance changed by a principal outside
   that authority fails, while owning-user control of a `PerUserDirect` installation is
   an explicit threat exclusion. Direct modes are `PerUserDirect` (default,
   user-owned LocalAppData tree), `MachineUacDirect` (explicit Program Files mode with
   Administrators/SYSTEM write and ordinary-user/Keld-role read/execute), and
   `MachineSeamlessDirect` (opt-in machine mode with SYSTEM-protected state and a
   separately approved narrow coordinator). Package/deployment-managed ownership is
   recorded as `Managed(owner)` and refuses Keld direct mutation. Missing, mismatched,
   unsupported or legacy-profile provenance refuses before feed access or filesystem
   mutation. A mode change requires an explicit trusted transition. Paths or executable
   names never infer mode or channel owner.
   A per-user install is protected from restricted Keld roles, but not from its owning
   user or arbitrary native code already running as that same user. Machine-wide ACLs
   do not claim protection from administrators.
   Before publishing provenance, the installer durably seeds `version-floor`,
   `current` and `last-known-good` to that same baseline artifact/version;
   provenance is the transaction's final commit record. Once direct provenance exists,
   a missing/corrupt floor fails closed even before the first update.
2. Given `updates.json` and its detached signature, verification follows
   architecture 06's existing order: literal-byte signature; duplicate-member-rejecting
   JSON; recognized schema; exact app/channel/target; strict SemVer; release versions
   unique by SemVer precedence; exact-duplicate delta `fromVersion` rejection; canonical
   positive `size`/`contentSize` integers in `1..=9007199254740991`; and valid
   digests. Invalid signatures, an unrecognized schema value, duplicate JSON members,
   equal-precedence releases, identity mismatch or malformed fields return a typed
   actionable refusal and activate no bytes.
3. Given a valid manifest, the existing persisted semantic-version floor filters for
   versions with SemVer precedence strictly greater than the floor and the client selects
   the single highest eligible version while retaining its complete version string as
   artifact identity. If filtering leaves no eligible release, return a typed successful
   no-update result without downloading, staging or mutating update state. A signed
   release at or below the installed baseline is ineligible. Malformed manifests and a
   missing/corrupt floor under direct provenance still fail closed; rollback never lowers
   the floor.
4. The first slice always downloads `full`, requires compressed and decompressed byte
   counts to equal their declared bounds, verifies both digests, validates the entire
   canonical archive before extraction, and writes archive entries only beneath protected
   `<version>/tree/`; retained `content.tar` and `.complete` are
   sibling updater metadata. A present delta entry has no effect. Exact ustar
   numeric/checksum/padding and complete-directory-entry golden vectors bind
   every producer and consumer to byte-identical `contentBlake3` input.
5. The first package cell is Windows x64 direct distribution and admits only packages
   representable by v0: regular files/directories, no links or special files, and the
   exact canonical metadata already defined in architecture 06. macOS/Linux and any
   link- or executable-mode-dependent package remain unsupported until an approved
   KEL-137 artifact replaces that limitation. Before any Windows write, the shared
   guard-owned classifier rejects separators/prefixes, ADS, Win32
   forbidden/control characters, reserved devices, trailing dot/space and tilde/8.3
   alias-shaped names; names must already be NFC, and the complete entry set must be
   unique under Windows ordinal case-insensitive comparison with no ancestor collision.
   The first producer for this cell runs on Windows because this exact admission uses
   native Windows normalization/comparison. A non-Windows producer host returns a typed
   unsupported-host result before output; cross-host assembly remains the target and
   requires independent equivalence evidence before that restriction is removed.
6. Before changing the trust floor or runnable pointer, the owner durably writes one
   activation journal containing a fresh attempt id, exact candidate
   `(app, channel, target, version, contentBlake3)`, validated rollback target,
   exact prior floor, prior last-known-good and previous-known-good artifacts. The order is journal
   `publish-pending`, trust floor, `current`, journal
   `awaiting-health`; each step is durable before the next begins.
7. After termination at every persisted boundary, recovery under the single-writer lock
   either resumes that exact journaled attempt, completes a journaled rollback, or halts
   for manual recovery. A malformed, replayed, mixed-artifact or pointer-inconsistent
   journal halts; directory presence and the floor never substitute for the journal.
   Recovery first proves the prior coordinator/candidate process family is gone. For
   `publish-pending`, current equal to candidate requires floor exactly equal to
   candidate and advances to `awaiting-health` without republishing. Current equal
   to rollback target permits only the recorded prior floor or exact candidate floor
   before resuming; every other combination, including floor above candidate, halts.
8. Candidate health is accepted only over a host-owned private channel minted for the
   journaled attempt. The receipt repeats the attempt id and artifact identity; the host
   must have booted from that exact version, reached application `Ready`, and
   remained alive for 30 monotonic seconds with no unexpected generation exit. A generic
   marker, prior receipt, different artifact, clean early exit, timeout, crash or lost
   channel cannot commit health.
   Candidate boot receives an inherited authenticated endpoint for this live attempt and
   enters read-only candidate mode: it verifies journal/current identity, does not
   acquire the writer lock or run orphan recovery, and cannot self-commit health.
9. The previous last-known-good pointer and package remain unchanged until exact health
   is durably recorded. The owner then journals `health-accepted`, moves the prior
   last-known-good to `previous-known-good`, publishes `last-known-good` to
   the candidate, retires the superseded older version (if any), and removes the journal.
   Failure journals `rollback-pending`, republishes `current` to the attempt's
   validated rollback target, retires the failed candidate, and only then removes the
   journal. Neither path lowers the trust floor; bounded cleanup retains both
   known-good slots.
10. If Windows requires a post-exit helper, the signed helper inherits only protected
    update-root/lock handles, the host-process wait handle, the observer/server endpoint
    and a sealed forward-once candidate endpoint for the already-minted health channel.
    It reads the exact attempt from the protected journal, waits for that host to exit,
    performs the journaled same-volume publish, passes only the sealed candidate endpoint
    in the explicit inherited-handle list, closes its copy after spawn, launches only
    the journaled executable, observes exact health, commits or rolls back, and exits.
    Command line, environment, cwd, caller paths and feed bytes convey no authority. The
    journal binds the verified helper image and health-channel identities; replay,
    endpoint substitution/reuse, helper substitution, mixed artifact set or path
    substitution fails.
11. Windows stages each complete version in a unique sibling directory, flushes every
    file handle, closes stage handles, then publishes the absent final directory with
    same-volume `MoveFileExW(MOVEFILE_WRITE_THROUGH)`. Journal/pointer records use
    same-directory temporary files, file flush and replace/write-through. Neither path
    uses copy-across-volume or claims a directory-handle flush. The implementation
    reopens and reads back every tree digest, journal, pointer and policy before
    advancing, and real crash cuts qualify the filesystem. Linux later uses file
    `fsync`, same-filesystem rename and parent-directory
    `fsync`; macOS later adds `F_FULLFSYNC` before rename plus
    directory synchronization. An unsupported barrier, remote filesystem or failed
    read-back makes that cell unsupported.
12. In every admitted direct mode, hostile app roles and webviews cannot write
    provenance, trust root, version floor, journal, staged package, pointers, helper
    input or install tree. The per-user updater and application host may share the
    ordinary user's OS identity; strict role restrictions must still deny Keld roles
    access to the user-owned updater state. Legacy same-user role mode refuses direct
    update because its token cannot be distinguished from the host. Administrators and
    arbitrary same-user native malware remain outside this boundary.
13. Every Slice-A package contains `.keld/update-policy.v1` with exact UTF-8
    bytes `{"schema":1,"dataMigration":"none"}\n`, covered by
    `contentBlake3`. Missing, duplicate or different policy refuses
    activation. Keld exposes no migration hook in this slice.
14. Disk-full, locked-file, interference, offline, corrupt download, signature failure,
    failed health, cleanup failure and concurrent-attempt fixtures preserve one
    diagnosable state. A failure before the version floor advances may retry the same
    signed version after repair. Once the floor advances, a launch/health-failed
    candidate remains ineligible for automatic reselection; a later automatic attempt
    requires a newly signed higher version. Removing all delta code/dependencies leaves
    the full-package updater complete.
15. A future delta path must verify reconstructed bytes against the selected release's
    `full.contentBlake3`, fall back once to `full` in the same
    attempt where safe, and retain the same journal, health, trust-floor and rollback
    contracts.
16. `PerUserDirect` is the default Windows installer choice. Installation and update
    state live under that user's LocalAppData-owned tree; the normal updater obtains its
    exclusive write lease as that ordinary user without UAC. Keld roles remain unable to
    write the state. Setup records the owner and mode before direct admission, provisions
    the stable activation-lease file with the owner-private descriptor, and seeds the
    same baseline/floor/current/LKG invariants as machine installation.
17. `MachineUacDirect` uses an installer-provisioned Program Files protection profile
    owned by BUILTIN Administrators, with a protected DACL granting full control only to
    BUILTIN Administrators and SYSTEM and read/execute (`0x1200A9`) to ordinary
    BUILTIN Users/Keld roles. The installer must be able to assign Administrators as a
    valid owner group in its actual token (`SE_GROUP_OWNER`, not deny-only); otherwise
    installation fails without owner/ACL takeover or repair. It provisions the stable
    activation-lease file and protected ancestors with this profile. Each activation that needs protected mutation requests UAC for a
    fixed signed updater helper; denial/cancellation causes zero protected writes. This
    UAC helper is the Machine-UAC authority adapter for the common verifier and
    transaction: it authenticates its bootstrap from the admitted host, obtains the
    lease, and independently revalidates the ordinary user's bounded cache input under
    retained read handles before any protected write. A user-side `VerifiedFull`
    receipt is not privileged proof. The production stage entry consumes an opaque
    mode-bound activation-write capability from this authenticated helper after it has
    freshly reloaded protected provenance and floor; a read-only `LoadedWindowsBaseline`
    or logical `AdmittedInstallation` alone cannot authorize protected staging. The
    helper creates a fresh protected sibling stage
    through the shared extraction/copy/readback pipeline; every directory and file has
    the exact Machine-UAC owner/DACL at creation, before payload bytes are written.
    It must not rename a user-owned stage into Program Files or seal a user-writable
    stage after the fact. Fake host, stale attempt, changed source bytes, a source with
    writable handles/mappings, or an unauthenticated endpoint fails before protected
    publication. Caller paths/argv/environment do not authorize mutation. The optional post-exit
    helper in criterion 10 is a different, narrower component; it only completes an
    already journaled attempt through inherited sealed handles and does not parse feeds
    or packages. The UAC helper owns the attempt while alive and launches the exact
    candidate under the initiating ordinary user's token/session (including
    over-the-shoulder approval), retaining exact-health/commit/rollback rules. If it exits
    or Windows restarts before resolution, recovery requires renewed UAC consent or
    halts with the journal unresolved; it must not claim an unproved rollback. The app
    and Bun roles never run elevated or as SYSTEM.
18. `MachineSeamlessDirect` is an explicit install-time opt-in. It preserves the same
    updater transaction and requires its coordinator to authenticate exact host,
    installation and fresh attempt; prevent replay; acquire installation-wide exclusive
    writer ownership; bind candidate health and process-family lifecycle; launch the
    application as the ordinary user; and recover safely across crash/reboot. The
    product mode is approved, but the Windows authority mechanism is not. Task Scheduler,
    a service or another Windows-native mechanism may be selected only after each named
    negative and positive control passes. A task start/trigger is never update
    authorization. No implementation of a privileged coordinator is authorized by this
    criterion alone. Its trusted initializer seeds the stable activation lease with the
    SYSTEM profile.
19. `Managed(owner)` provenance makes `keld-update` return a typed owner-delegation
    result before fetching, staging or mutating. MSIX/App Installer, Store and enterprise
    deployment remain the only update authorities for their managed installation.
20. Each direct mode uses one trusted-installer-created regular `activation.lock` file
    under the protected update root. It is persistent, never deleted/recreated/replaced,
    and its presence does not identify a live owner; do not reuse KEL-266's
    `bootstrap.lock`. A writer opens the existing file read/write with exclusive sharing
    (share mode zero), no reparse following or inheritance. A snapshot reader opens it
    read-only with `FILE_SHARE_READ` only and holds that short lease for one coherent
    mutable-record snapshot. Normal startup retains only immutable selected-tree and
    ancestry pins after releasing mutable journal/floor/current/LKG record pins. The
    writer retains the exclusive lease while the candidate performs its authenticated
    bootstrap read; the candidate closes mutable-record pins and acknowledges bootstrap
    before application execution, retaining only immutable selected-tree pins and its
    health endpoint through the 30-second window. Sharing conflict is a typed busy/refusal with no retry or
    sleep. Missing, wrong-kind, wrong-volume or wrong-profile lock state fails closed.
    On Windows machine-wide profiles, ordinary-user read access to this lease also lets
    a local native process hold a conflicting share-mode handle and deny update
    availability until that handle closes. This is an availability-only residual: the
    updater must fail closed before protected writes, and no integrity claim depends on
    successful lease acquisition. The contract does not promise update availability
    against hostile local native users. This follows from the documented
    [CreateFile sharing rules](https://learn.microsoft.com/windows/win32/api/fileapi/nf-fileapi-createfilew).

## 4. Design

### Atomic decomposition and first-principles decision

| Atom / owner | Boundary and input → output | Failure mode | Independent observable |
|---|---|---|---|
| Feed wire / architecture 06 | signed v0 bytes → one highest eligible full release | equal-precedence releases, noncanonical/unbounded sizes or ambiguous parsing | immutable manifest bytes/signature fixtures with SemVer and numeric boundary mutations |
| Channel provenance / installer + host | protected receipt → direct admission or refusal | path heuristic mutates managed install | substitution table before feed/write counters |
| Package / `keld-pack` | full artifact → one Windows v0 tree/policy | unsupported link/mode or hidden migration | canonical bytes and hostile archive corpus |
| Trust / `keld-update` | verified version → monotonic semver floor | rollback lowers/bypasses floor | floor trace independent of runnable pointer |
| Activation / `keld-update` | candidate + prior LKG → commit or rollback | partial publish or directory inference | crash cut after every durable transition |
| Health / candidate host | private attempt channel → exact receipt | stale/generic/mixed receipt commits | field substitutions and kill/early-exit controls |
| OS durability / adapter | bytes + barriers → durable read-back | prerequisite missing after crash | native barrier failure and crash-cut matrix |
| Helper / Windows package | journal + inherited handles → post-exit publish | arbitrary path, replay or mixed set | independently substitute every helper input |
| User data / package owner | signed no-migration policy → rollback eligibility | binary rollback after migration | absent/changed policy refuses pre-launch |
| Evidence / task owner | exact source + OS receipts → task artifact | mock/stale head closes native row | exact-head provenance validator |
| Install mode / trusted installer + KEL-53 | explicit mode choice → immutable owner/protection provenance | path/environment selects authority | substitute path/env/registry; refuse before feed/write |
| Writer authority / mode adapter + `keld-update` | one exact attempt → one temporary exclusive write lease | UAC/start trigger mistaken for attempt authentication or concurrent writer admitted | wrong authority and competing writer leave journal/pointers unchanged |

The exact portable `.keld/update-policy.v1` path and bytes are owned once by `keld-pack`;
`keld-update` depends on that producer-side owner and checks the authenticated archive
entry byte-for-byte without JSON reserialization. An independent literal golden vector
prevents producer/consumer agreement on an incorrect constant. `keld-pack` has no
dependency on `keld-update`.

The manifest authenticates candidate bytes but does not own channel provenance, health or
local recovery. The trust floor decides future eligibility but never says which binary
is healthy. The journal owns only the in-flight attempt; `current`,
`last-known-good` and `previous-known-good` own their selections.

**Reuse:** keep the v0 detached signature, strict semver floor, canonical tar, version
directories and single-writer lock. Slice A ignores delta entries and adds no dependency
or manifest field.

**Rejected alternatives:** a generic health marker can outlive its candidate; advancing
last-known-good before health destroys the rollback target; path inference can mutate a
store-owned install; directory reconstruction accepts partial/mixed attempts; lowering
the floor reopens replay; an un-hashed policy permits substitution.

Compatibility fallback: unsupported packaging/channel/filesystem cells keep the current
version and report the missing predecessor or owning update mechanism.

**Target boundary:** `keld-update` remains the common transaction and recovery owner;
the admitted host remains the only attempt/health identity minter. The host creates the
attempt health channel and the candidate receives only its attempt-bound endpoint. The
mode-specific adapter obtains the one temporary write lease for that attempt: the
ordinary user-owned updater for `PerUserDirect`, an explicitly elevated signed helper
for `MachineUacDirect`, or a still-unselected narrow coordinator for
`MachineSeamlessDirect`. `Managed(owner)` obtains no Keld write lease. Every adapter
executes the same journal, floor, pointer, health, commit, rollback and crash-recovery
transitions. UAC/task/service wake-up is not attempt authentication. A helper may own a
live attempt only for the bounded health/commit/rollback lifecycle; it cannot mint
identity or survive as a general updater.

**Cross-owner contract:** KEL-135 owns verified publisher/app/profile identity. KEL-53
alone mints `ActivePackageSelection` from journal/current/LKG state and owns install-mode
provenance, update journal and the sole writer. KEL-254 owns OS verification of
installed-image and protection profiles but never selects current/LKG or mints an active
selection. KEL-96/core consumes that exact `ActivePackageSelection`, performs host boot
admission and returns its opaque `ValidatedBootSelection` to the ordinary-user host.
None of the consumer/identity owners infers install mode or mutates updater state.
KEL-254's approved installed-root spec defines the per-user and Machine-UAC protection
profiles; their native implementation and verification remain required before those
direct modes ship.

### Internal state and transition contract

These internal shapes are not public Rust API or manifest wire:

```rust
struct ArtifactIdentity {
    app_id: CanonicalAppId,
    channel: Channel,
    target: Target,
    version: StrictSemver,
    content_blake3: [u8; 32],
}

struct ActivationAttempt {
    attempt_id: [u8; 32],
    candidate: ArtifactIdentity,
    coordinator_image_blake3: [u8; 32],
    health_channel_id: [u8; 32],
    lifecycle_channel_id: [u8; 32],
    rollback_target: ArtifactIdentity,
    prior_floor: StrictSemver,
    prior_last_known_good: ArtifactIdentity,
    prior_previous_known_good: Option<ArtifactIdentity>,
}

enum ActivationPhase {
    PublishPending(ActivationAttempt),
    AwaitingHealth(ActivationAttempt),
    HealthAccepted(ActivationAttempt, HealthReceiptDigest),
    RollbackPending(RollbackAttempt),
}

struct RollbackAttempt {
    attempt_id: [u8; 32],
    target: ArtifactIdentity,
    prior_current: ArtifactIdentity,
    expected_floor: StrictSemver,
    expected_last_known_good: ArtifactIdentity,
    expected_previous_known_good: Option<ArtifactIdentity>,
    coordinator_image_blake3: [u8; 32],
    health_channel_id: Option<[u8; 32]>,
    lifecycle_channel_id: [u8; 32],
    cause: FailureClass,
}
```

The journal is a strict versioned local record. Unknown versions, duplicate fields,
noncanonical values and pointer/artifact mismatches fail closed. The host generates the
random `attempt_id`; config, roles, environment and feed cannot supply it.
The journal also carries a fresh `lifecycle_channel_id`, distinct from both the
attempt and health identities. A stable rendezvous locator is derived from trusted,
immutable installation identity and user/session scope, so a cold successor can
discover the keeper without reading mutable journal data while the keeper holds the
share-zero lease. The locator is not authorization and MUST NOT disclose secrets. After
peer authentication, the keeper supplies the bound attempt ID and both sides bind a
one-use challenge/ack to install, attempt, peer process and connection generation.

The lifecycle `installation_id` is owned by `keld-update`; callers MUST NOT mint it
from a path, random value, command-line value or separate installer assertion. The
updater derives its 32 bytes as
`BLAKE3(UTF8("keld.installation-binding/provenance-v2/v1\0") || u64_le(n) || p)`,
where `p` is the exact canonical encoded `keld.install-provenance/v2` record and `n`
is its byte length. This binds the explicit mode, owner, app/channel/target, install and
update roots, signing-key identity, baseline artifact, profile, principal model,
publisher scope and volume. The running host derives the expected ID from trusted
packaging configuration; recovery recomputes it only after reading protected provenance
and matching every field to that configuration. Equal provenance yields a stable ID;
relocation changes the ID. This digest is binding context, not a secret or peer
authentication. A future provenance schema MUST preserve an explicitly defined v2
projection or introduce a separately versioned lifecycle-ID derivation.

Before any Job witness or lease-retention capability moves, each side MUST authenticate
the actual connected named-pipe peer and same-session profile. The health token is
never reused for keeper recovery or exposed to app roles.

The lifecycle rendezvous MUST NOT treat the existing bearer-token `HELLO`, a pipe DACL,
PID, SID or signed publisher alone as peer authentication. Both ends retain the actual
connected pipe and process objects, verify exact permitted role/image plus token user,
session and integrity, bind the independently expected install and attempt, and reject
exit or identity changes during acquisition. The client sends a fresh nonce; the server
returns a fresh challenge and the client acknowledges the full transcript. These values
prove freshness/liveness only; OS peer identity authenticates. The client MUST set
`SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION` when opening its pipe to the server.
The server validates the last HELLO writer's thread token through
`ImpersonateNamedPipeClient`/`TOKEN_QUERY`, always reverting impersonation. Client and
server pipe handles MUST be non-inheritable
and MUST NOT be duplicated to app roles. If the connected endpoint can be inherited or
duplicated to an untrusted role, PID queries do not establish who wrote the message and
the mechanism refuses to transfer handles. A one-shot generation is consumed before
any capability transfer; simultaneous successors, stale attempt IDs and transcript
replay all refuse. If `RevertToSelf` fails, the process MUST terminate immediately and
MUST NOT return to the accept/retry path.

The lifecycle pipe uses the dedicated `\\.\pipe\keld-lifecycle-<64 lowercase hex>`
namespace, disjoint from ordinary `\\.\pipe\keld-<64 lowercase hex>` app-link pipes.
The two clients MUST reject the other namespace before opening a pipe; the endpoint
prefix is the protocol discriminator before the distinct nonce handshake. After the
client/server nonce exchange, the server sends one fixed-size 177-byte `KELD-LC1`
binding challenge; the client returns `KELD-LA1` only after validating it against its
independent expectation. The server consumes its one-shot listener before sending the
same-context `KELD-LR1` acceptance receipt. The server returns an admitted peer only
after the receipt write completes before its deadline; the client returns only after
verifying that receipt before its deadline. The record is: 8-byte magic,
1-byte purpose, 32-byte
installation ID, 32-byte attempt ID, 32-byte lifecycle-channel ID, 32-byte client
nonce, 32-byte server nonce, 4-byte client PID and 4-byte server PID (PIDs little
endian). Purpose is a closed tag for coordinator-to-keeper or keeper-to-successor.
The client MUST compare install and purpose before acknowledgement. A cold successor
may learn attempt/channel only from the authenticated keeper, and MUST mark those IDs
as unverified until it reacquires the writer lease and revalidates them against the
protected journal; it MUST NOT mutate journal or pointers on a mismatch. Both peers
MUST validate the actual connected process objects and fresh nonce pair. Mismatch,
wrong purpose, stale exact-attempt expectation, malformed/truncated challenge or
acknowledgement/receipt MUST fail closed before any handle transfer. `LC1`/`LA1`/`LR1` version this
dedicated lifecycle subprotocol; ordinary app-link frames are unchanged.

After LR1, the coordinator sends one exact 89-byte `KELD-HO1` bundle on the same
consumed connection: 8-byte magic, coordinator-to-keeper purpose byte, 32-byte
attempt ID, 32-byte lifecycle-channel ID, 8-byte target-process Job handle and
8-byte target-process activation-lock handle (handle values little endian). The
duplicated Job has only `JOB_OBJECT_QUERY | JOB_OBJECT_TERMINATE`; the lease handle
has only `FILE_READ_ATTRIBUTES | SYNCHRONIZE`. The keeper adopts the exact process
handles, re-attenuates both, rejects inherited/wrong-object handles, and independently
queries a nonzero Job family before sending the exact 76-byte `KELD-HR1` receipt
(magic, attempt ID, channel ID, active count). The coordinator retains its local Job
and writer-lease owners until HR1 matches; a failed transfer keeps local ownership
and halts. If a malformed/truncated HO1 leaves remote handle values unknown, the
dedicated keeper MUST terminate before returning to any caller so process teardown
closes those handles. After delivery may have begun, the coordinator MUST NOT close
remote handles by their saved numeric values: the keeper may already have closed and
reused those slots. Instead it terminates and waits for the exact pinned keeper while
retaining its own Job/lease handles.

After the pinned coordinator exits, the keeper terminates the exact Job and MUST
observe zero before sending a query witness over the one-shot successor connection. Its 81-byte
`KELD-QO1` record contains magic, keeper-to-successor purpose, attempt ID, channel ID
and the remote query-only Job handle. The successor independently queries zero and
sends a 76-byte `KELD-QA1` receipt with the same attempt/channel and zero count. Only
after exact QA1 does the keeper close its activation-lock retention and send the
matching `KELD-QF1` final receipt. The successor returns from the lifecycle API only
after QF1, then acquires the writer lease and revalidates install, journal attempt
and channel before mutation. Before QA1 the keeper MUST retain the lock and halt on
failure. Observing Job zero alone does not authorize lease release: the successor
must cross the explicit QA1 gate. If QA1 succeeds but QF1 delivery is lost, the
successor MUST still halt without journal/pointer writes; another writer without the
retirement witness also refuses. The records carry handles only over the authenticated, one-use connection;
PID/name discovery never substitutes for a retained process object or Job handle.

Primary API contracts: [GetNamedPipeClientProcessId](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeclientprocessid),
[GetNamedPipeServerProcessId](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeserverprocessid),
[ImpersonateNamedPipeClient](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-impersonatenamedpipeclient),
and [DuplicateHandle](https://learn.microsoft.com/en-us/windows/win32/api/handleapi/nf-handleapi-duplicatehandle).

**Lease and snapshot ownership:** every direct installation receives a trusted-installer-
created stable `activation.lock` regular file under `update_root`. The file is never
deleted/recreated/replaced; existence is not live-owner evidence. A writer opens this
existing object read/write with share mode zero, no reparse following or inheritance.
Normal selection/recovery readers open read-only with `FILE_SHARE_READ` only and hold
that shared lease only while reading one coherent set of mutable records. A sharing
conflict is busy/refusal, without sleep or retry. The writer lease remains held through
the active attempt and health window. The authenticated candidate-boot path is the only
reader exception: while the coordinator keeps journal/current stable, it reads the exact
attempt and current briefly, closes mutable-record handles and acknowledges bootstrap
completion before app execution/health can permit record replacement. It retains only
immutable selected-version/tree pins and its attempt endpoint during the health window.
Normal host selection closes mutable journal/pointer/floor pins after snapshot and keeps
only the selected immutable artifact pins. Lock presence never decides whether the
previous coordinator/candidate family is live; recovery still requires the independent
process-family owner. `bootstrap.lock` remains only the KEL-266 initializer marker.

The process-family owner is an independent input to the common transaction: an
activation lease or a valid journal cannot prove an earlier host/candidate is gone.
On Windows, KEL-96/runtime owns ordinary-user host admission and process-family
observation; KEL-53 consumes that exact result before recovery or replacement. The
current `keld dev` outer-Job plus host-inner-Job tests prove developer-session cleanup
ordering only; they do not establish an installed updater launcher or a
MachineSeamless writer mechanism.

Windows recovery MUST NOT interpret `OpenJobObject` not-found as
`ActiveProcesses == 0`. A temporary object name is removed when its handle count
reaches zero even while kernel references may remain; a persisted Job name or attempt
ID is only a locator while an authenticated keeper still retains the exact handle.
Recovery requires a live retained-handle zero observation or separately qualified,
durable retirement evidence. Without either, it halts and preserves the journal and
all pointers. A supported user-mode boot-epoch proof has not been established.
Source: [Windows object life cycle](https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/life-cycle-of-an-object).

**Bounded per-attempt lifecycle keeper (proof slice; no production activation writes).**
The first Windows implementation slice MUST use the exact unnamed attempt Job object
and transfer/duplicate its handle; opening a Job by persisted name is forbidden. The
keeper runs outside that Job and receives only the least rights needed to terminate
the exact family and query its active count (`JOB_OBJECT_TERMINATE` and
`JOB_OBJECT_QUERY`); it receives no assign, limit-change, file-write or transaction
command authority. It retains only the reduced, non-writable duplicate of the stable
`activation.lock` object needed to keep the existing writer exclusion alive, plus the
exact coordinator process observation needed to distinguish coordinator death from
pipe loss. KEL-53 remains the only writer of journal, floor and pointers.

The keeper may terminate and observe the exact Job only after the authenticated
coordinator process object is signaled. Writer exclusion is not retired when a pipe
closes, a Job name disappears, or a PID is absent. Retirement requires a fresh,
attempt-bound, one-shot authenticated handoff of the exact Job witness. The selected
successor independently queries that witness and acknowledges the exact zero result
over the authenticated connection. The keeper releases its lock-retention handle only
after that acknowledgement; the successor then acquires the exclusive writer lease and
revalidates the protected install and journal attempt. Reacquiring before release is
impossible while the share-zero retention handle remains open. A competing writer that
wins after release cannot mutate a pending attempt without the matching retirement
witness and MUST refuse recovery. No authenticated handoff or proof means journal and
all pointers remain unchanged and recovery halts.

Adversarial proof MUST separately cover coordinator death, keeper death, both owners
lost before handoff, wrong Job/host, stale and replayed attempts, competing writer,
exact Job-family zero, and fake endpoint. The lifecycle code must not be connected to
production pointer/floor/activation/commit writes until these controls pass. Reboot,
hibernate and all-owners-lost recovery remain unsupported unless a documented Windows
mechanism proves durable retirement; otherwise ambiguous recovery halts with evidence
preserved.

The single-writer transition is:

1. verify protected direct provenance and mode, acquire the mode-supplied stable lease,
   then verify/extract `full`, including `.complete` and policy;
2. retain and validate current as the rollback target plus both known-good slots;
3. persist `PublishPending`;
4. advance the semantic-version trust floor;
5. publish `current` to the candidate;
6. persist `AwaitingHealth` and launch with a private health channel;
7. on exact health, persist `HealthAccepted`, publish the prior LKG to
   `previous-known-good`, publish `last-known-good` to candidate, retire the
   superseded older version (if any), then remove the journal; deleting retired trees is
   best-effort cleanup that never touches either known-good slot;
8. on failure, persist `RollbackPending`, publish `current` to the
   validated rollback target, retire the failed candidate, remove the journal, then
   report failure.

Retirement is the only mutation of a published version. While the journal still
authorizes it, the writer renames the one unreferenced version directory, with the
same-parent absent-target write-through adapter, to a generated `retired-<64 hex>`
name. Generated `incomplete-*` and `retired-*` names can never equal a SemVer version
directory, so the census admits them only as never-selectable diagnostics. Retiring
under the journal closes the crash window in which a removed journal would leave an
orphan complete version that halts every later writer. On NTFS an open file handle
anywhere in the retiring tree, with any sharing mode, makes the rename fail; the journal
then stays for journal-bound recovery. A process still executing a mapped image from
that tree does not block the rename: it keeps running from the retired tree, cannot
open further files by their original path, and its tree is deleted only after it
exits. A launched candidate is therefore retired only after process-family retirement,
and the host must not run other instances from a superseded version.
Journal removal is a write-through rename of the journal to a generated `pending-*`
leaf followed by deletion. A crash before any record sibling's publication rename
leaves only such a `pending-*` file. The census admits `pending-*` names; before its
first write, the next transaction removes each one only after verifying a regular,
single-link file with the exact installation profile, and refuses anything else.
A refusal of a new attempt before its `PublishPending` journal exists retires every
version that attempt published, so a refused start leaves no orphan unless that
retirement itself fails, which reports `UnjournaledVersionRetained`. A process crash
in that window still leaves an orphan; the ordinary loader halts on it, and only the
explicit unjournaled-version repair, admitted when no journal exists and every record
validates, retires it under the writer lease. The repair first verifies and pins every
referenced version, removes stale `pending-*` record siblings, and admits for retirement
only strict-SemVer entries whose completion record names that version in the
installation's scope; any other unknown or damaged entry, including a non-directory under a
generated name, refuses the repair before any rename and needs manual recovery.

A `PublishPending` journal is resumable under the exclusive writer lease alone: no
candidate is launched before `AwaitingHealth` is durable, and every live transaction
owner retains the share-zero lease (or its keeper retains a duplicate), so acquiring
the lease proves no prior owner can still write and no candidate family exists. Every
resumed owner durably re-mints the attempt's health and lifecycle channel identities
before continuing, so no health receipt or retirement witness from a lost owner binds
to the resumed run. Launched phases still require an exact process-family retirement
binding. That binding is an exact installation/attempt/lifecycle-channel value that the
host composes from the QF1 retirement witness or its own retained Job-zero observation;
`keld-update` cannot authenticate its producer. A sealed witness type was rejected
because it would add a `keld-update` -> `keld-runtime` edge outside the approved crate
graph.

`HealthAccepted` records `BLAKE3(UTF8("keld.activation-health-receipt/v1\0") ||
attempt_id || health_channel_id || u64_le(n) || a)`, where `a` is the canonical
artifact-identity encoding of the candidate and `n` its byte length. The receipt binds
the attempt, its private health channel and the exact candidate; recovery recomputes
the digest from the journal's own fields and halts on a mismatch.

Windows replaces fixed mutable record slots through a narrow same-parent
`MoveFileExW(MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)` adapter; the existing
absent-target version-directory publication remains separate. A transition writes a
new protected sibling with the mode's exact DACL, flushes and reads it back, closes
conflicting record pins, performs the fixed replacement under the stable lease, then
reopens/reads back the exact bytes and descriptor before the next step. It does not
truncate in place, copy across volumes, schedule work after reboot, or accept caller
paths/flags. A replacement error is effect-aware; it does not claim the old bytes stayed
unchanged after a possible publication.

Startup without a journal validates current, both known-good slots, their complete
markers/policies and the floor. A valid current must equal last-known-good or
previous-known-good; any other complete artifact is an orphan and halts (generated
`incomplete-*` and `retired-*` diagnostics are not artifacts). If current is
invalid but last-known-good is valid, recovery republishes last-known-good. A
missing/invalid last-known-good after installation halts even when current runs;
previous-known-good may be absent only before the first successful update. Recovery
acquires the attempt lease and proves the prior
coordinator/candidate family exited. Valid `PublishPending` with current still at
the rollback target accepts only the recorded prior floor or exact candidate floor
before resuming. With current already at the candidate it requires floor exactly equal
to candidate and advances to `AwaitingHealth` without republishing. Every other
combination, including floor above candidate, halts. `AwaitingHealth` rolls back
only after the process-family
proof. `HealthAccepted` finishes both known-good publications and the superseded
version's retirement. `RollbackPending` finishes rollback and the candidate's
retirement only after floor, both known-good slots,
coordinator/helper identity, optional health identity and current exactly match its
recorded context. Corrupt or mixed state halts without deleting evidence.

The launched candidate receives the client end of the live attempt channel through the
platform's protected inherited-handle mechanism. That endpoint selects candidate boot
mode before ordinary updater startup: validate exact attempt/current/artifact, skip the
writer lock and orphan recovery, start the app, and report boot/Ready/health to the
coordinator. Missing, replayed or mismatched bootstrap fails before app code. Normal
startup has no such endpoint and follows the recovery path above.

### Trust, package and channel ownership

The semver floor is the only v0 replay/downgrade floor. It advances before candidate
publication and never rolls back. `current` may point below it after health
failure; that is intentional local rollback, not permission to reinstall an old release.

The installer synchronizes the immutable baseline package, seeds floor/current/LKG to
that exact artifact, then creates immutable provenance as its final commit. `Direct`
records exact identity/channel/target/roots/key/baseline, install mode, owner and its
mode-specific OS protection profile. `Managed(mechanism)` always refuses direct
mutation. Missing provenance is unsupported. Location, registry heuristics and
writable config never manufacture a mode or `Direct` ownership.

`PerUserDirect` is the default and stores the application and updater state beneath
the owning user's LocalAppData tree. Its updater uses the ordinary user identity and
does not display UAC; strict Keld role restrictions deny role access to update state,
while the security claim excludes the owning user and arbitrary native malware running
as that user. `MachineUacDirect` stores machine-wide state in a Program Files-style root
whose installer-provisioned DACL allows elevated Administrators and SYSTEM to mutate,
and ordinary users/Keld roles only to read/execute. It is not KEL-266's SYSTEM-only
profile and never repairs ownership/DACL during an update. `MachineSeamlessDirect` is an
opt-in SYSTEM-protected profile whose narrow writer mechanism remains a separate proof
gate. `Managed(mechanism)` performs no direct feed, stage or protected write.

The product and ownership boundary is explicit: KEL-135 owns verified publisher/app/
profile identity; KEL-53 owns install-mode provenance, update journal, active-package
selection and the sole writer; KEL-254 owns OS verification of installed-image and
protection profiles plus the read-only selection boundary, never write authority;
KEL-96 owns host boot admission and consumes the exact opaque selection as the ordinary
user, without inferring install mode or mutating updater state. KEL-254's approved
installed-root spec defines the per-user and Machine-UAC protection profiles; their
native implementation and verification remain required before those direct modes ship.

Windows x64 direct distribution is first because its tree fits v0. KEL-137 is an
explicit predecessor for macOS/Linux or any package requiring metadata absent from v0.

### T3b: protected Windows extraction (KEL-265)

This task produces an unpublished, incomplete stage, not an activated version. It
does not supply the installer provenance loader, live strict-profile admission,
persisted-floor loader or T4 publication/recovery. The first filesystem cell is
Windows x64, fixed local NTFS with persistent ACLs and no read-only volume flag.
Failed/unknown qualification refuses; there is no pathname or filesystem fallback.
`GetVolumeInformationByHandleW` observes the retained root, and
`GetFinalPathNameByHandleW(VOLUME_NAME_GUID)` supplies the exact volume root for
`GetDriveTypeW`. These observations do not prove physical non-removability or
power-loss durability.

The host-facing API has opaque, non-cloneable owners:

```rust
impl AdmittedInstallation {
    pub fn open_windows_extraction_root(&self)
        -> Result<WindowsExtractionRoot, UpdateError>;
}
impl WindowsExtractionRoot {
    pub fn extract<'a>(&'a mut self, verified: &VerifiedFull, archive: &Path)
        -> Result<ExtractedWindowsStage<'a>, UpdateError>;
}
```

The root opener derives `update_root` from the admitted installation and creates
nothing. It retains no-follow directory handles through every accepted absolute
path component, with delete sharing disabled. Both `update_root` and its existing
`versions` directory must be actual non-reparse directories with the current
TokenUser owner and exactly one protected, non-inherited OI/CI full-control allow
ACE for that user. Missing scaffolding or a different descriptor refuses; extraction
does not repair ACLs or initialize installation state. Logical
`ProvenanceObservation::Protected` is not evidence for these OS facts.
Existing installer path components use the guard's ordinary filesystem grammar,
including serviceable tilde names. The stricter package namespace grammar applies
to archive members and newly created package directories, not ancestor root names.

`SelectedFull` and `VerifiedFull` privately retain the admitting
`DirectInstallationIdentity`. Before opening the source or creating a stage,
extraction reuses the provenance owner's complete identity comparison, including
key, roots, baseline and profile, and requires candidate SemVer precedence above
the receiving root's admitted floor. Equivalent verifier instances may interoperate;
different admission contexts may not. This floor is a snapshot: T4 must reread the
protected floor under its single-writer lock before publication.

The source is an internally opened, unexposed read-only file with only read sharing.
It must be regular, non-reparse and single-link. Retain it across complete canonical,
namespace, policy and digest preflight and every source read; do not combine a
detached preflight receipt with a caller-controlled reader. Complete preflight
precedes creation. A fresh unpredictable stage name is created exclusively beneath
retained `versions`; collisions refuse rather than reusing or repairing an object.

Directory creation uses one private, directory-only `NtCreateFile` adapter with a
retained `RootDirectory`, one validated component, `FILE_CREATE`,
`FILE_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT`,
`OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE`, no handle inheritance and no delete
sharing. A shared owner-private descriptor is supplied atomically. The successful
handle passes metadata and ACL checks before any descendant creation. Safe pinned
capability file operations provide create-new/no-follow regular files beneath those
retained parents; every file is checked for type, reparse state, single link and the
exact inherited current-user full-control ACL before its first write.

`keld-guard` becomes the single owner of the existing CLI/core owner-private
descriptor construction and validation. Core retains its public path wrapper and
CLI retains atomic dev-stage creation; neither retains a second ACL policy.
The updater reuses this owner and the guard package namespace validator.
`cap-std`/`cap-fs-ext` file primitives are reused. Their Windows mkdir resolves a
pathname, so it cannot establish confinement when an ancestor's reparse attributes
are mutable. Delete-sharing pins alone do not restrict attribute or ACL changes.
The narrow native directory adapter replaces only that missing primitive, using
pinned `windows-sys` bindings rather than a copied ABI or generic filesystem layer.

Retained `content.tar` is a sibling of `tree`; archive paths address only `tree`.
Stream bounded chunks, flush every written file, close its writer, reopen it relative
to its retained parent and read it back against authenticated source bytes. Verify
object identity, length and protection on readback and retain read handles with no
write/delete sharing, together with every created directory, for the stage lifetime.
The returned stage borrows the exclusive root owner and exposes identity/diagnostic
name only, not raw handles or a trusted pathname capability. It creates no `.complete`,
final version name, floor, journal or pointer. T4 must consume the stage and perform
the separately qualified publication sequence. Any failure after stage creation
returns a typed error naming the incomplete stage; preserve that bounded diagnostic
state without recursive cleanup or a success receipt.

The independent atoms are: admission identity (context substitution refuses before
source/stage I/O), byte authentication (locked-source mutation refuses), authorization
(permissive root/versions and hostile LPAC writes refuse), containment (single-component
relative create cannot affect an outside sentinel), lifecycle (flush/readback failure
never returns a stage), and evidence (real Windows handles/token plus exact source).
ACL protection and retained-object lifetime are an explicit composition edge, not
interchangeable proofs. Tests include matched successful mutation controls, source
writer/mapping conflicts, hardlink/reparse/collision refusal, released-pin rename,
and temporary mutations of context, ACL, relative-create and readback checks. No
sleep synchronization and no shipping installer/activation claim arise from fixtures.

The implementation may add existing workspace-pinned `cap-std`, `cap-fs-ext`,
`windows-sys`, `windows-permissions` and `getrandom` consumer edges, plus Windows-only
test dependencies for existing LPAC launch primitives. It introduces no crate/version,
manifest field, application grant or KIPC change. The updater's exact new unsafe path
requires its own scoped instruction owner and independent review; no crate-wide
unsafe permission is granted.

### T4a: protected Windows baseline bootstrap (KEL-266)

This bounded amendment implements the machine-baseline portion of acceptance criterion 1 before activation. The
installer is a one-shot, externally provisioned LocalSystem process; the ordinary
host receives read authority only. It adds no service, elevation path, recovery,
health, activation selection or role grant. KEL-254 still owns installed boot and
must compare these protected publisher facts with KEL-135's independently verified
current-image identity. Its fresh-role read provisioning remains separate. This is the
KEL-266 `MachineSeamlessDirect` baseline predecessor only; it does not define the
default per-user bootstrap or the distinct Machine-UAC ACL/helper contract.

| Atom / owner | Boundary and observable contract | Independent falsifier |
|---|---|---|
| Identity / updater | Trusted installer/host configuration binds installation, publisher scope and volume GUID | Change one expected field; no receipt |
| Authentication / manifest and full verifier | Literal signed manifest selects the exact configured baseline and fully verifies its package | Wrong signature, full version string or digest; zero publication |
| Authorization / guard | Actual initializer TokenUser is SYSTEM; caller configuration is trusted deployment input, never an elevated feed/argv assertion | Ordinary or elevated non-SYSTEM token refuses |
| Persistent containment / guard | Every ancestor excludes ordinary-user replacement between invocations | Protected leaf under user-owned parent or parent DELETE_CHILD refuses |
| Lifetime containment / updater | Retained no-delete-share ancestors and relative creation bind live objects | Reparse, hardlink, rename, source-write and readback controls |
| Lifecycle / initializer | Exclusive fresh lock; exact initial seed state; provenance is last commit record | Existing state, competing initializer or pre-provenance crash produces no admitted installation |
| Evidence / native tests | Real token, descriptor, filesystem and subprocess observations | Logical fixtures never satisfy SYSTEM/user or crash-cut acceptance |

Signature verification does not prove writer authority; authority does not authenticate
arbitrary configuration; a matching record does not prove OS protection; completed I/O
does not alone prove power-loss durability. These atoms stay independently testable.

The first cell is Windows x64 on one qualified fixed local NTFS volume. Trusted
`WindowsBaselineTrust` holds the existing `DirectInstallationIdentity`, the existing
32-byte KEL-135 publisher scope and canonical volume-GUID root. Records encode that scope
as exactly 64 lowercase hexadecimal characters. Publisher scope is an installer
assertion, not an Authenticode result. It must originate in trusted deployment/host
configuration, not lower-trust environment, feed or arguments. The loader compares all
fields, including observed volume identity; the protected record cannot supply its own
expected trust anchor. SYSTEM/admin volume restoration is outside the ordinary-user
replay threat; no global monotonic counter is introduced.

Supported paths are lossless UTF-8 absolute drive paths, optionally verbatim-drive,
with normal guard-validated components; UNC, device aliases, reparses, relative/dot
components, empty components and alternate separators refuse. `update_root` is one
direct child of `install_root`; `versions` is a direct child of `update_root`.
The externally provisioned initial install root contains only that update directory,
which contains only empty `versions`. Install/update/versions already have the exact
`windows-system-users-rx-v1` MachineSystem descriptor. No initializer creates, seals
or repairs the scaffold; it validates these committed roots before mutation.
Every earlier named ancestor below the volume root already has the committed machine
descriptor. No user-owned intermediate path (for example a development workspace) is
an admitted installation location.

The shared guard owns the committed profile `windows-system-users-rx-v1`: owner SYSTEM,
protected DACL, exactly SYSTEM full control plus BUILTIN Users file read/execute
(`0x1200a9`); directory ACEs have object/container inheritance, file ACEs have none.
Every object is explicitly sealed and read back; inheritance alone is not proof.
The volume anchor has a separate conservative predicate: trusted SYSTEM,
Administrators or TrustedInstaller owner; present DACL with only understood ordinary
ACE forms; effective allow ACEs for other trustees grant at most read/execute plus
creation of new directories. No untrusted DELETE, DELETE_CHILD, WRITE_DAC, WRITE_OWNER,
WRITE_DATA, WRITE_ATTRIBUTES, WRITE_EA or generic-write/all right is admitted.
Inherit-only entries do not grant access to the anchor; all descendants are independently
checked. The helper reports an unsupported anchor rather than changing a drive ACL.
Both initializer and loader validate persistent ancestry before trusting state and
retain opened components for the result lifetime. T3b owner-private policy is unchanged.

`BaselineVerifier` reuses the existing literal-signature/strict-manifest parser and
full/archive verification. Its separate opaque `SelectedBaseline`/`VerifiedBaseline`
receipts select the exact configured baseline version string and content digest without
inventing an admitted installation, protected observation or lowered floor. Ordinary
update selection still chooses only the highest release strictly above its floor.
Reuse T3b's source locks, canonical/policy parser, relative directory adapter, extraction,
flush and readback; do not add a second archive, signing or policy implementation.

One crate-private bounded (64 KiB) canonical UTF-8 JSON codec owns local records. Each
record has a distinct explicit v1 schema; unknown/duplicate/missing fields, unsupported
schemas and bytes differing from typed reserialization refuse. `install-provenance`
under the install root records direct ownership, protection profile, complete existing
installation identity, publisher scope and volume GUID. `.complete` records exact
artifact identity and content size. Under the update root, `version-floor` records
the exact baseline version, while `current` and `last-known-good` record the complete
baseline artifact. No previous-known-good or activation journal exists initially.
The version directory name is the complete validated baseline version string.

The only successful initialization order is:

1. Prove actual SYSTEM authority, topology, volume, exact MachineSystem descriptors and
   fresh state; create `bootstrap.lock` exclusively with its exact MachineSystem
   descriptor at object creation and retain its handle. Existing lock/state refuses;
   no PID guessing, takeover or stale cleanup. Seed the persistent `activation.lock`
   with that same exact descriptor and exclusive sharing.
2. Fully validate the authenticated exact baseline before extraction; create one fresh
   incomplete sibling and populate it with shared T3b mechanics.
3. Seal/read back each owner-private stage directory bottom-up. Payload files and
   `content.tar` receive their final MachineSystem descriptor before the original
   writable handle's final flush and protected readback. Create `.complete` with its
   final MachineSystem descriptor, write and flush it, then verify its exact bytes and
   descriptor.
4. Close rename-blocking stage handles while retaining protected ancestors. Publish to
   the absent final version name using same-volume `MoveFileExW` with only
   `MOVEFILE_WRITE_THROUGH`; no replacement or cross-volume-copy flags. Reopen and
   validate all content, policy, marker, descriptors and exact extracted-tree bytes.
5. Seed floor, current and LKG in order: each uses a fresh same-parent temporary file,
   final protection, writable-handle flush, close, absent-target write-through rename
   and protected readback. Any conflicting target or incomplete prior state refuses.
6. Revalidate the unchanged install/update/versions descriptors and retained lock; do
   not rewrite already trusted scaffold ACLs. Publish protected provenance by that same
   file procedure LAST. Re-read through the production loader and separately validate
   the exact complete initial seed state before returning success.

Failures retain diagnostic incomplete state and never silently reseed. The bootstrap
lock may remain after a commit; it is not activation/recovery authority. The read-only
loader returns coherent initial-baseline identity/floor and retained read handles. It
neither chooses an active package nor grants mutation, recovery, live strict-profile
or role authority by itself.
The initializer's exact-baseline postcommit tree check is separate from future active
package selection and garbage collection.

**Admission-to-staging completion (KEL-266 AC4–6).** Before exposing its observation
to the existing updater verifier, the public baseline loader also validates the exact
baseline `version-floor`, `current`, `last-known-good`, protected baseline version
directory and matching `.complete`. Missing/corrupt/mixed metadata refuses there,
not only in the initializer's final check. Previous-known-good, journal or unknown
update-root state belongs to future activation/recovery and refuses this initial cell.
The persistent bootstrap lock may remain, but is never repair authority. Reuse/factor
the existing seed and completion-record owners; retain the pointer, marker and version
directory handles. This metadata admission does not rehash the entire runnable tree
or authenticate the current executable. The latter remain their existing owners.

The only additional names admitted under `versions` are diagnostic
`incomplete-<64 lowercase hexadecimal characters>` siblings. Validate each named
object as a non-reparse directory without following or selecting its contents; it is
never a runnable artifact or a source of identity/floor/completion. Other final
versions require the future activation predicate rather than directory inference.

`LoadedWindowsBaseline::into_windows_extraction_root(self)` consumes the real retained
loader owner and requires the actual SYSTEM token. It derives installation/floor and
the versions handle internally, accepting no caller identity, floor, root or logical
protected observation. One private closed extraction-authority variant retains this
machine owner; the existing T3b owner-private variant is unchanged. Conversion and the
extraction mutation boundary require actual SYSTEM and the exact committed machine
descriptor. The consumed loader's metadata/ancestor pins live through extraction.
Both variants call the same verifier, source locking, namespace and copy/readback
implementation with owner-private stage protection. No `.complete`, final version,
floor, pointer, journal, repair or activation is produced by this conversion/staging.

Native acceptance starts from real initialized state: existing `UpdateVerifier`
admission consumes the qualified observation, authenticates a signed higher full
release, and SYSTEM stages its exact bytes while every baseline record remains
byte-identical. Ordinary-user conversion refuses before source/stage I/O. Another
installation/key/profile receipt, a non-higher candidate or changed machine descriptor
refuses. Release every owner and prove that the only new output is a private incomplete
stage. Canonical wrong pointer/marker/floor values and missing records must make the
public loader refuse; valid metadata restored byte-for-byte is the positive control.

The real LPAC probe runs under an ordinary host after releasing installation handles.
Reuse runtime launch/token observation and the existing filesystem-probe owner; grant
read/execute to its disposable helper and write authority only within its disposable
role-private control directory, granting neither to the committed machine installation.
Verify known provenance, floor/current/LKG and
payload targets exist, exercise actual denied mutations, require granted role-private
controls to succeed, then re-read protected bytes/descriptors. This proves the exercised
LPAC write denial, not installed-role read provisioning or WebView2 acceptance.

Native tests cover actual non-SYSTEM refusal, SYSTEM success, standard-user reads and
write/WRITE_DAC/rename denial after all initializer handles close, parent substitution,
wrong publisher/volume, extra write ACE, changed marker/tree/seed records, concurrent
initialization and subprocess termination at every persisted boundary. The current
unelevated agent cannot claim SYSTEM acceptance: a concrete reviewed operator helper
must run it. API completion plus process crash cuts is not a power-loss claim; the
governing native filesystem qualification remains required before shipping this cell.

This adds no dependency version, manifest/KIPC field or app permission. Existing pinned
Windows APIs are reused. Only the fixed write-through absent-target publication adapter
extends updater production unsafe ownership, with independent unsafe/permission/public
API and local-record protocol review. No new guard unsafe is authorized. Rejected
alternatives are a same-user/elevated-owner writer (implicit owner WRITE_DAC), a general
broker/service (unneeded authority), copied signer/parser code (duplicate owner), and
logical protected fixtures as native proof. No existing public compatibility fallback
changes; unsupported native cells return typed actionable refusal.

### Capabilities, wire and errors

Application permissions cannot grant update authority. No KIPC, renderer bridge or
permission-manifest change is introduced.

Wire changes: none. `updates.json`, its detached signature, v0 fields and
delta parsing remain unchanged. Slice A chooses `full`.

`keld-update` owns typed provenance, verification, package, activation-state,
health, durability and recovery categories. `keld-pack` owns production
failures. CLI/compat render them without remapping. Exact `KELD-UPDATE-*`
codes land with emitting code because the registry rejects un-emitted codes.

## 5. Boundaries

Implement in:

- `keld-update`: provenance, verification, floor, journal, pointers, health and
  recovery;
- `keld-pack`: Windows v0 package and exact no-migration policy;
- existing host/runtime owners: private health channel and candidate lifecycle;
- trusted installer: immutable per-user/machine-UAC/machine-seamless/managed mode
  provenance and owner-specific protection profile;
- a minimal signed helper for explicit-UAC activation and, if separately approved, for
  locked-file publication;
- existing doctor/build diagnostics and native fixtures.

Must not touch in Slice A:

- KIPC frames, renderer bridge or permission syntax;
- v0 manifest fields/meaning;
- delta dependencies/algorithms;
- store/package-manager mutation APIs;
- application databases or a migration engine;
- macOS/Linux activation before KEL-137 and native qualification.

## 6. Tasks

- [x] T1 — synchronized this approved multi-mode contract with Architecture 06, regenerated
  included docs, and independently reviewed the exact cross-document head. Direct-mode
  implementation follows this contract; the product decision does not authorize a
  seamless privileged mechanism.
- [ ] T2 — v0 manifest/full verifier plus protected provenance admission/refusal; no
  delta dependency.
- [ ] T3a — produce canonical Windows x64 v0 full packages on a Windows host, with the
  exact no-migration policy owned by `keld-pack` and byte-checked by `keld-update`; add
  independent ustar/policy golden vectors, producer-to-verifier digest/size agreement,
  invalid-name zero-write controls and non-Windows typed refusal. This producer-only
  task does not write extracted files.
- [ ] T3b — after T3a, add two-pass protected Windows extraction beneath the admitted
  staging root; retain guard-owned case/NFC/namespace rejection, hostile archive corpus,
  and real reparse/rename substitution refusal before any write.
- [ ] T4a — KEL-266: actual SYSTEM initializer, protected exact-baseline seeds,
  provenance-last publication, stable activation-lease seeding, read-only loader and
  persistent ancestry proof; no activation.
- [ ] T4b — common Windows x64 direct transaction: journal, floor/current/LKG order,
  attempt-bound 30-second health, one mode-supplied write lease, and crash cut at every
  persisted boundary; no per-mode state machine fork. Progress: the common transaction
  (production journal, fixed-slot write-through replacement, one step function for
  forward progress and recovery, exact receipt/retirement/coordinator binding and
  version retirement) passes PerUserDirect subprocess crash cuts at every persisted
  boundary. The host-owned private health channel, 30-second `Ready` observation and
  installed-host QF1 composition remain.
- [ ] T4c — default per-user install/bootstrap and no-UAC authority; prove v2 owner/mode
  provenance, stable lease seeding and hostile-role write denial under the user's LocalAppData tree.
- [ ] T4d — explicit-UAC Program Files authority; prove the installer token can assign
  BUILTIN Administrators as owner (`SE_GROUP_OWNER`, not deny-only), exact protected
  ancestor/state DACLs and canonical descriptors on published records; filtered-token
  denial-zero-write; authenticated helper bootstrap, exact candidate revalidation,
  initiating-user candidate launch and live-owner health/rollback;
  after owner death/reboot recovery obtains fresh consent or safely halts.
- [ ] T4e — machine-seamless product row remains gated: prove exact host/install/attempt
  auth, replay resistance, writer/read-pin handoff, family lifecycle, ordinary candidate,
  exact health and crash recovery before selecting or implementing any native mechanism.
- [ ] T5 — managed-owner refusal, hostile-role denial, locked file/disk/interference/
  concurrency and next-attempt recovery.
- [ ] T6 — after KEL-137, repeat independently for each macOS/Linux format/channel.
- [ ] T7 — separately approve signed data compatibility/migration before a migrating
  release can use automatic binary rollback.
- [ ] T8 — only after the baseline, measure optional delta reconstruction while retaining
  the full fallback.

## 7. Test plan

| Criteria | Proof and falsifier |
|---|---|
| 1, 11–12, 16 | provenance/mode/channel/profile/ACL table and installer seed crash cuts; mode/path/owner substitution refuses before feed/write; per-user installer uses LocalAppData with no UAC and actual hostile-role write denials |
| 2–4 | signed v0 fixtures, duplicate-member parser, equal-precedence build-metadata release pair, floor selection including equal-precedence/different-metadata and below-baseline replay, numeric mutations (`0`, `-1`, fraction, exponent, `2^53 - 1`, `2^53`), shorter/exact/longer compressed and decompressed byte counts, digest boundaries and complete ustar golden bytes; selecting a present delta fails Slice A |
| 5, 13 | independent canonical Windows tar/policy goldens; producer-to-verifier size/hash agreement; missing/duplicate/changed policy refusal; link/special/mode mismatch, omitted/duplicate parent directory, separator/ADS/device/forbidden/control/trailing-dot/NFC/case/8.3 aliases and ancestor collisions reject before output; T3b separately tests extraction-order and filesystem reparse/rename substitution |
| 6–7, 9 | state trace and subprocess crash after every durable step, including current published before phase advance; floor above candidate, non-prior intermediate floor, orphan no-journal current and mixed rollback context halt; live/unknown coordinator blocks recovery; corrupt/replay/mix every journal field |
| 8 | live-coordinator candidate boot skips writer-lock recovery; stale attempt/artifact, coordinator death, early exit, crash, timeout and generic marker fail; exact Ready plus 30 monotonic seconds passes |
| 10–11, 17 | real Windows locked-file/helper, staged-directory publish and same-volume barrier/read-back crash cuts; elevated installer assigns Administrators owner only when TokenGroups has SE_GROUP_OWNER and not deny-only; exact protected owner/DACL read-back on ancestors and records; filtered medium token and second ordinary user are denied write/create/delete/rename/WRITE_DAC/WRITE_OWNER while read succeeds; SYSTEM/Admin writer controls succeed; at AfterStageCreate/BeforeFileFlush, the same account's filtered medium token cannot create/write/obtain WRITE_DAC on Machine-UAC stage objects; UAC denial, fake host, stale attempt, changed source bytes or fake endpoint cause zero protected publication; over-the-shoulder candidate remains in initiating ordinary token; live helper owns health/rollback; actual admitted Keld roles fail mutations |
| 18 | mechanism-neutral seamless row: wrong host/role/image/token profile/install, fake endpoint, peer exit during acquisition, inherited/duplicated pipe-handle leak, stale/replayed attempt, simultaneous successors, competing writer/read-pin race, live/unknown process family and crash/reboot controls; no task/service chosen without every row passing |
| 19 | trusted MSIX/App Installer/Store/enterprise provenance returns typed defer before network/feed/stage/write; direct updater creates no competing writer |
| 20 | real Windows stable `activation.lock` remains present across release/crash; multiple short read leases coexist and block the writer; exactly one share-zero writer is admitted after readers close; missing/wrong-profile lock refuses; a surviving child cannot be mistaken for a dead process family; candidate closes mutable-record pins and acknowledges bootstrap before app code/health, while immutable selected-tree pins remain held; writer replaces mutable records during candidate health without replacing/deleting pinned immutable trees |
| 14 | deterministic fault injection followed by one successful attempt; delta code absent |
| 15 | future base/patch/reconstructed-content mutations and same-attempt full fallback |

No sleep synchronization. State tests inject transitions; process tests wait on
handles/events with bounded kill switches. Windows evidence records filesystem, build,
source SHA, package/signature identity and raw crash cuts. Other OS results are separate.

## 8. Review gates triggered

- unsafe: none in this contract; conditional on each exact native/helper implementation;
- public API: yes — canonical package contents, update admission and unsupported-cell
  diagnostics are author-facing contracts;
- permission model: yes — the install-mode protection profiles, UAC elevation and
  hostile-role denial decide who can mutate executable state, though no app grant is added;
- dependency addition: none;
- wire protocol: yes — v0 bytes stay unchanged, but Slice-A delta-selection semantics
  and canonical package content are narrowed and require exact independent review; any
  new host/coordinator authentication channel remains separately owned and gated.

## 9. Perf impact

No improvement claim. Slice A records download bytes, staging footprint, activation and
health latency. The 30-second window reuses KEL-70's default crash-window duration but
is stricter: any unexpected generation exit fails health. It bounds commit latency
without delaying candidate launch. A future delta must report CPU, memory, bytes,
fallback rate and end-to-end success before adding complexity.

## 10. Open questions

The product mode selection is approved; the following are implementation/evidence gates,
not requests to revisit that decision:

- T4b's common journal and persisted recovery have real Windows PerUserDirect
  crash-cut evidence; the host-owned attempt-bound health channel, 30-second `Ready`
  observation and installed-host lifecycle composition remain open.
- T4c must prove the default per-user install root, mode/provenance seeding and actual
  role write denial; the owning user's authority remains outside the threat claim.
- T4d must prove the Administrators/SYSTEM ACL, UAC cancellation with zero writes,
  over-the-shoulder user-token launch, and exact health/rollback under the live elevated
  owner. Reboot/owner death requires new consent or safe halt.
- T4e must close every host/attempt/authentication/replay/writer/lifecycle/health/recovery
  falsifier before any privileged seamless mechanism is selected. The task probe is only
  wake-up feasibility.
- KEL-254's installed-image consumer and KEL-96's host admission must consume the exact
  updater selection under each admitted direct-mode profile without acquiring updater
  write authority. KEL-135 remains sole publisher/app/profile identity owner.

Manifest/full verification, logical provenance admission, Windows packaging and protected
incomplete extraction have landed. T4a initialization and every activation/mode cell
retain their separate native acceptance; this specification does not claim them shipped.
