# Spec: reliable full-package activation before delta optimization

Status: approved
Linear: KEL-53 · Owner: GYLDLAB · Updated: 2026-09-26
Approval: original corrected contract: Linear comment `b343d835-1528-461f-bda4-0fa5e238b5af` · approved corrected content head `a340acf0b5cfcbfab9111f938cd3ac2788219ccb` · decision SHA-256 `972b82947189b5d89c7c78d11547f0c0ef890a60bf36af1bdfb68d448fed2ed9`; KEL-263 producer-host and policy-owner amendment: delegated approval comment `df61a6f6-3215-44a8-8780-7ae242fc74ab` · decision SHA-256 `871a803ba4c04087209ebb7a19751a15382dcae42cd8764d2cd7090d3cc9ba83` · approved content head `3d5d51e31237c365847a1e9b866880ba06429e2e`; complete crate dependency graph: supplemental delegated decision comment `212a2039-f729-4708-a983-9a348d299bd5` · decision SHA-256 `939cc2100ba25b172088371c1311e7555156c0f8e9430dd8ad38998caa5d0fc8` · approved content head `3d5d51e31237c365847a1e9b866880ba06429e2e`.

{"schema":"keld.kel53-approval/v1","decision":"approved","approved_content_head":"a340acf0b5cfcbfab9111f938cd3ac2788219ccb","approver_id":"49ccfebb-c3fb-40a3-abb2-a3bf92e83cb1","linear_comment_id":"b343d835-1528-461f-bda4-0fa5e238b5af","source":"active-maintainer-session-2026-09-20"}
{"schema":"keld.kel53-approval/v1","decision":"approved","approved_content_head":"3d5d51e31237c365847a1e9b866880ba06429e2e","approver_id":"49ccfebb-c3fb-40a3-abb2-a3bf92e83cb1","linear_comment_id":"df61a6f6-3215-44a8-8780-7ae242fc74ab","source":"delegated-maintainer-session-2026-09-26"}
{"schema":"keld.kel53-approval/v1","decision":"approved","approved_content_head":"3d5d51e31237c365847a1e9b866880ba06429e2e","approver_id":"49ccfebb-c3fb-40a3-abb2-a3bf92e83cb1","linear_comment_id":"212a2039-f729-4708-a983-9a348d299bd5","source":"delegated-maintainer-session-2026-09-26"}

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

KEL-266 AC4–6 completion: delegated approval comment
`bfeb14d0-e906-476f-970a-7fd837bc7f2f`, approved content head
`a7d54066704f08cb170435ad72877afdea93f6d1`, file SHA-256
`d43431e186fa20f1ca3d2281dfab06ef9fb8227be5044face730c3390b6896fb`.
This supplement closes coherent admission, higher-release staging and committed-state
LPAC evidence within the same issue; it does not claim activation or installed boot.

## 1. Goal & non-goals

Keld's direct updater must first prove one safe signed full-package
install/activation/recovery path. The first admitted cell is a Windows x64
direct-distribution package whose complete file tree fits the existing v0 canonical
archive. The updater verifies one signed release, stages exact bytes, publishes one
attempt-bound candidate, and either commits its exact health receipt or restores the
retained last-known-good package. Delta patches remain a later optional transport
optimization.

Non-goals:

- no delta algorithm or dependency in the first slice;
- no updater for package-manager/store-owned installs;
- no macOS/Linux package claim before KEL-137 supplies the executable-mode/link
  representation those cells require;
- no data-migrating release, migration hook, or claim that binary rollback rolls data
  back in the first slice;
- no arbitrary relaunch helper, shell command, self-update plugin, or role-writable
  update state;
- no TUF-style rotating-root design beyond the existing v0 single-key limitation;
- no implementation before this approved corrected specification lands.

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

## 3. Acceptance criteria (binary, each becomes a test)

1. Given a direct-distribution installation, startup consumes an installer-created,
   OS-protected provenance record naming the exact app id, channel, target, install
   root, update root, installed baseline artifact and compiled-in signing-key identity.
   It also binds the admitted strict/distinct-OS-principal security profile. A missing,
   mutable, mismatched, legacy-profile or package-manager/store-owned record refuses
   direct update before feed access or filesystem mutation. Paths or executable names
   never infer channel owner.
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
   the candidate, and removes the journal. Failure journals
   `rollback-pending`, republishes `current` to the attempt's validated
   rollback target, and only then removes the journal. Neither path lowers the trust
   floor; bounded cleanup retains both known-good slots.
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
12. In an admitted strict/distinct-OS-principal package, hostile app roles and webviews
    cannot write provenance, trust root, version floor, journal, staged package,
    pointers, helper input or install tree. Legacy same-user role mode refuses direct
    update because its token cannot be ACL-distinguished from the per-user host.
    Administrators and arbitrary same-user native malware remain outside this boundary.
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

**Target boundary change:** `keld-update` remains the crash/recovery owner and the
host remains the only principal minter. The host creates the attempt health channel;
the candidate receives only its attempt-bound endpoint. If needed, the Windows helper
inherits the observer endpoint, a sealed forward-once candidate endpoint, borrowed
update-root/lock handles and one host-process wait handle after trusted spawn. It
temporarily owns the journaled activation through health/rollback, then exits; it
cannot mint identity or survive as a general updater. No live boundary changes in this
spec-only PR.

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
    cause: FailureClass,
}
```

The journal is a strict versioned local record. Unknown versions, duplicate fields,
noncanonical values and pointer/artifact mismatches fail closed. The host generates the
random `attempt_id`; config, roles, environment and feed cannot supply it.

The single-writer transition is:

1. verify protected direct provenance and acquire the update lock;
2. verify/extract `full`, including `.complete` and policy;
3. retain and validate current as the rollback target plus both known-good slots;
4. persist `PublishPending`;
5. advance the semantic-version trust floor;
6. publish `current` to the candidate;
7. persist `AwaitingHealth` and launch with a private health channel;
8. on exact health, persist `HealthAccepted`, publish the prior LKG to
   `previous-known-good`, publish `last-known-good` to candidate, remove the
   journal, then clean retention without deleting either known-good slot;
9. on failure, persist `RollbackPending`, publish `current` to the
   validated rollback target, remove the journal, then report failure.

Startup without a journal validates current, both known-good slots, their complete
markers/policies and the floor. A valid current must equal last-known-good or
previous-known-good; any other complete artifact is an orphan and halts. If current is
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
proof. `HealthAccepted` finishes both known-good publications.
`RollbackPending` finishes rollback only after floor, both known-good slots,
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
that exact artifact, then creates the protected provenance record as its final commit.
`Direct` records exact identity/channel/target/roots/key/baseline and admitted
strict/distinct-OS-principal profile identity.
`Managed(mechanism)` always refuses direct mutation. Missing provenance is
unsupported. Location, registry heuristics and writable config never manufacture
`Direct`.

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

This bounded amendment implements acceptance criterion 1 before activation. The
installer is a one-shot, externally provisioned LocalSystem process; the ordinary
host receives read authority only. It adds no service, elevation path, recovery,
health, activation selection or role grant. KEL-254 still owns installed boot and
must compare these protected publisher facts with KEL-135's independently verified
current-image identity. Its fresh-role read provisioning remains separate.

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
which contains only empty `versions`. Install/update/versions initially have the exact
existing SYSTEM-private descriptor. No initializer creates or repairs the scaffold.
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

1. Prove actual SYSTEM authority, topology, volume, descriptors and fresh state; create
   `bootstrap.lock` exclusively with create-new under the private update root and retain
   its handle. Existing lock/state refuses; no PID guessing, takeover or stale cleanup.
2. Fully validate the authenticated exact baseline before extraction; create one fresh
   incomplete sibling and populate it with shared T3b mechanics.
3. Seal/read back every stage object. For payload files and `content.tar`, establish
   final protection on the original writable handle before its final file flush and
   protected readback; directory sealing remains bottom-up. Write `.complete` last within the stage, flush its
   writable handle, seal it and verify its final bytes and descriptor.
4. Close rename-blocking stage handles while retaining protected ancestors. Publish to
   the absent final version name using same-volume `MoveFileExW` with only
   `MOVEFILE_WRITE_THROUGH`; no replacement or cross-volume-copy flags. Reopen and
   validate all content, policy, marker, descriptors and exact extracted-tree bytes.
5. Seed floor, current and LKG in order: each uses a fresh same-parent temporary file,
   final protection, writable-handle flush, close, absent-target write-through rename
   and protected readback. Any conflicting target or incomplete prior state refuses.
6. Seal/read back install/update/versions and the retained lock. Publish protected
   provenance by that same file procedure LAST. Re-read through the production loader
   and separately validate the exact complete initial seed state before returning success.

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
- a minimal signed Windows helper only if locked-file acceptance requires it;
- existing doctor/build diagnostics and native fixtures.

Must not touch in Slice A:

- KIPC frames, renderer bridge or permission syntax;
- v0 manifest fields/meaning;
- delta dependencies/algorithms;
- store/package-manager mutation APIs;
- application databases or a migration engine;
- macOS/Linux activation before KEL-137 and native qualification.

## 6. Tasks

- [ ] T1 — promote this spec with architecture 06 synchronization, generated docs and
  exact-head review. No implementation starts until this approved corrected specification
  lands.
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
- [ ] T4a — KEL-266: actual SYSTEM initializer, protected exact-baseline seeds and
  provenance-last publication, read-only loader and persistent ancestry proof; no activation.
- [ ] T4b — Windows x64 direct vertical: journal, floor/current/LKG order, attempt-bound
  30-second health and crash cut at every persisted boundary.
- [ ] T5 — Windows helper if required, managed-channel refusal, hostile-role denial,
  locked file/disk/interference/concurrency and next-attempt recovery.
- [ ] T6 — after KEL-137, repeat independently for each macOS/Linux format/channel.
- [ ] T7 — separately approve signed data compatibility/migration before a migrating
  release can use automatic binary rollback.
- [ ] T8 — only after the baseline, measure optional delta reconstruction while retaining
  the full fallback.

## 7. Test plan

| Criteria | Proof and falsifier |
|---|---|
| 1, 11–12 | protected provenance/channel/profile/ACL table and installer seed crash cuts; legacy mode refuses before feed/write; mutate every identity/root/owner/floor and attempt hostile-role writes |
| 2–4 | signed v0 fixtures, duplicate-member parser, equal-precedence build-metadata release pair, floor selection including equal-precedence/different-metadata and below-baseline replay, numeric mutations (`0`, `-1`, fraction, exponent, `2^53 - 1`, `2^53`), shorter/exact/longer compressed and decompressed byte counts, digest boundaries and complete ustar golden bytes; selecting a present delta fails Slice A |
| 5, 13 | independent canonical Windows tar/policy goldens; producer-to-verifier size/hash agreement; missing/duplicate/changed policy refusal; link/special/mode mismatch, omitted/duplicate parent directory, separator/ADS/device/forbidden/control/trailing-dot/NFC/case/8.3 aliases and ancestor collisions reject before output; T3b separately tests extraction-order and filesystem reparse/rename substitution |
| 6–7, 9 | state trace and subprocess crash after every durable step, including current published before phase advance; floor above candidate, non-prior intermediate floor, orphan no-journal current and mixed rollback context halt; live/unknown coordinator blocks recovery; corrupt/replay/mix every journal field |
| 8 | live-coordinator candidate boot skips writer-lock recovery; stale attempt/artifact, coordinator death, early exit, crash, timeout and generic marker fail; exact Ready plus 30 monotonic seconds passes |
| 10–11 | real Windows locked-file/helper, staged-directory publish and same-volume barrier/read-back crash cuts; substitute every inherited endpoint/input |
| 14 | deterministic fault injection followed by one successful attempt; delta code absent |
| 15 | future base/patch/reconstructed-content mutations and same-attempt full fallback |

No sleep synchronization. State tests inject transitions; process tests wait on
handles/events with bounded kill switches. Windows evidence records filesystem, build,
source SHA, package/signature identity and raw crash cuts. Other OS results are separate.

## 8. Review gates triggered

- unsafe: none in this spec; conditional for platform/helper implementation;
- public API: yes — canonical package contents, update admission and unsupported-cell
  diagnostics are author-facing contracts;
- permission model: yes — protected provenance/update authority and hostile-role denial
  decide who can mutate executable state, though no app grant is added;
- dependency addition: none;
- wire protocol: yes — v0 bytes stay unchanged, but Slice-A delta-selection semantics
  and canonical package content are narrowed and require exact independent review.

## 9. Perf impact

No improvement claim. Slice A records download bytes, staging footprint, activation and
health latency. The 30-second window reuses KEL-70's default crash-window duration but
is stricter: any unexpected generation exit fails health. It bounds commit latency
without delaying candidate launch. A future delta must report CPU, memory, bytes,
fallback rate and end-to-end success before adding complexity.

## 10. Open questions

None in the technical contract. Approval is bound to the content heads and Linear
receipts recorded above. Manifest/full verification, logical provenance admission,
Windows packaging and protected incomplete extraction have landed. T4a implementation
requires its own native acceptance; activation, health, recovery and the later tasks
remain separate work. This task list does not claim those unfinished paths are shipped.
