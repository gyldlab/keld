# Spec: reliable full-package activation before delta optimization

Status: draft
Linear: KEL-53 · Owner: GYLDLAB · Updated: 2026-09-19
Prior approval (superseded for this corrected draft): Linear comment `aba995f0-74fc-4696-b15d-0fa2cbee3775` · approved content head `e44e21cace974cc33e8176c94b599491f31cb918` · decision SHA-256 `3fddfd72070062d9b024e3029bf8561d43728ff5590e5bfe5c9311f166d4e288`

Previous approval payload: `{"schema":"keld.kel53-approval/v1","decision":"approved","approved_content_head":"e44e21cace974cc33e8176c94b599491f31cb918","approver_id":"49ccfebb-c3fb-40a3-abb2-a3bf92e83cb1","linear_comment_id":"aba995f0-74fc-4696-b15d-0fa2cbee3775","source":"active-maintainer-session-2026-09-20"}`

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
- no implementation before the corrected specification is approved and lands.

## 2. Spec refs

- `docs/architecture/06-runtime-and-tooling.md` §4/§4a owns the v0 feed,
  manifest, canonical archive, verification order, local trust floor and activation
  model.
- `docs/architecture/03-security.md` owns update trust, protected state and the
  narrow Windows relaunch-helper boundary.
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

This proposal keeps the v0 JSON fields but intentionally revises their validation,
client-selection and canonical-package semantics before any updater implementation
exists. Final approval of the corrected exact content head re-approves the v0 public
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
  exact-head review. No implementation starts until the corrected specification is
  approved and lands.
- [ ] T2 — v0 manifest/full verifier plus protected provenance admission/refusal; no
  delta dependency.
- [ ] T3 — after the shared KEL-130 Windows component classifier exists, consume it in
  Windows v0 package production; add exact ustar golden vectors, two-pass extraction,
  case/NFC/namespace collision rejection, no-migration policy and hostile archive corpus.
- [ ] T4 — Windows x64 direct vertical: journal, floor/current/LKG order, attempt-bound
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
| 5, 13 | canonical Windows tar/policy bytes; add link/special/mode mismatch, omitted/duplicate parent directory, separator/ADS/device/forbidden/control/trailing-dot/NFC/case/8.3 aliases, extraction-order collisions, sibling-metadata names, or omit/change policy |
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

None in the technical contract. The prior exact-head approval is preserved above but is
superseded for this corrected draft by the pre-merge review fixes. The updater remains
unimplemented; implementation requires renewed human approval bound to the corrected
exact content head and then the specification landing.
