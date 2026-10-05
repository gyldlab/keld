# Spec: authenticated Windows installed-root boot
Status: approved
Linear: KEL-254 · Owner: GYLDLAB · Updated: 2026-09-30
Prior exact draft approval: Linear comment `82b31e46-c970-4e92-89a6-7c7ec2484ee2` at
head `0867931a5478adc71ecb69ec4982782d35a83f34`, file SHA-256
`8bc3123e3f8cb8d568a87aea7b6b82d583e7bb7a7287c1bf1132571340d4f7d2`. That approval
applies to the prior bytes only. The multi-mode revision was approved by Linear comment
`e0b276f9-5ecc-42a9-ac8f-8a5e05f44245`, binding PR #290 head
`b78c89b061049647421dda69034c9f35079d9441` and the approved spec-content SHA-256
`c00c670aa23a8f26a8705e0f2a64899c823d959a17dc4d20c41ce9bd4c1c2ec9`.
Amendment A3 (installed-boot discovery and boot states): owner decisions recorded in the
active maintainer session on 2026-10-05; exact-content approval of this revision is
pending. A3 makes the current executable path a locator only, keeps exactly two admitted
Windows boot states, and names one build-time producer of the expected app identity
(§3 AC1–AC2, §4 "Selection shape (workspace export)", §6 T2b–T3).
It adds no install mode, no
production `unsafe`, and no record field. It changes Windows only: macOS and Linux keep
their current standalone and lease-less boot behavior and tests (see AC2).

## 1. Goal & non-goals

Allow a standard Windows user to start a directly installed Keld app after the host
proves that the executable, app identity, install-mode provenance, and boot files all
belong to the selected package. The default is a per-user install with seamless updates
and no UAC. Also support Program Files installs updated either through explicit UAC or,
only after a separate proof gate, a narrowly privileged seamless mechanism. Keep one
KEL-53 updater state machine and vary only the authority that obtains its write lease.
Keep the current owner-private dev-stage path unchanged and fail closed before creating
a listener, child, or window when any installed-package fact is absent or mismatched.

Non-goals:

- no weakening or removal of the existing Windows dev-stage DACL;
- no mode selection from `%ProgramFiles%`, executable location, environment, cwd,
  command-line data, a read-only bit, or a caller-supplied release flag;
- no claim that Authenticode on `keld-host.exe` authenticates neighboring files;
- no resistance claim against administrators or arbitrary same-user native malware;
- no direct writer or competing updater for MSIX, App Installer, Store, enterprise,
  package-manager, or other deployment-owned installs; their owner remains authoritative;
- no claim that a standard-user process can write a Program Files install;
- no approval of a Task Scheduler task, service, or other seamless privileged mechanism
  before the KEL-270 proof gate passes;
- no claim that the same-user per-user install protects files from arbitrary native code
  running as that same Windows user;
- no KIPC, permissions-manifest, updater-feed wire, or WebView2 profile-format change.

## 2. Spec refs

- `docs/architecture/03-security.md` §§1, 4, 5: host trust root, default-deny, OS
  protection, and update provenance boundaries.
- `docs/architecture/06-runtime-and-tooling.md` §§2, 3, 4, 4a: boot ownership,
  Windows x64 canonical package, signed full-artifact identity, and unsupported-cell
  refusal.
- `docs/specs/kel96-no-flag-host-boot.md` §§3, 4.1 D4, 4.2, 4.3, 4.8: strict boot
  descriptor, no caller-selected mode, and current dev-stage-only consumer. This spec
  is a proposed successor to D4 only for Windows direct-installed packages.
- `docs/specs/kel53-full-package-activation.md` §§3, 4, 5, 7, 8: installer-owned
  provenance, baseline/current/LKG package identity, candidate journal and endpoint,
  update-owner refusal, and real Windows evidence. Current implementation status is
  recorded below and in Architecture 06; activation/recovery is still a predecessor.
- `docs/specs/kel135-persistent-profile-identity.md` §§3, 4, 7: current Windows
  Authenticode identity, publisher/app separation, and identity-derived LocalAppData
  WebView2 profile.
- `docs/specs/kel102-host-guard-enforcement.md`: one-read permissions verifier and
  exact bytes.

This successor does not change the four-unique architecture or add a trust principal.
This exact spec revision and its install-mode contract are approved. The implementation
PR MUST keep architecture 03/06 and the KEL-96 D4 applicability note synchronized; the
D4 note remains outside the frozen decision block so its approved digest does not change.

## 3. Acceptance criteria (binary, each becomes a test)

1. Given a valid authenticated private dev lease and the current owner-private stage,
   when the host boots, it follows the existing `DevStage` validation path byte-for-byte
   in behavior. The installed-root path is not consulted and the dev DACL predicate is
   not weakened.
2. Given no valid dev lease, when the Windows host considers installed mode, then it
   obtains the KEL-135 `ValidatedAppIdentity` from the current executable's
   single-primary Authenticode verification and asks the KEL-53 owner to validate
   OS-protected direct-install provenance. KEL-53's journal-free active selection has
   landed; installed boot remains unavailable until the executable-located discovery
   entrypoint (§4) and this consumer are implemented and qualified. Missing, managed,
   corrupt, unprotected, or mismatched provenance is a typed refusal; signature success
   alone never admits installed mode and it never falls through to dev-stage or
   source-config boot. Windows admits exactly two boot states: a valid authenticated dev
   lease selects `DevStage`; otherwise only authenticated installed provenance selects
   the installed package; anything else fails closed. A staged layout launched without a
   valid dev lease is not a dev-stage boot, and there is no third signed-dev-stage state.
   Existing KEL-135 acceptance rows that launch a signed host from a lease-less dev stage
   move in T3: rows testing dev semantics receive a real dev lease, and rows testing
   installed identity or profile semantics move to installed fixtures. macOS and Linux
   keep their current standalone and lease-less boot behavior, and their tests are not
   migrated, because neither has an authenticated installed-root provenance successor
   yet. That is a temporary applicability difference, not a permanent trust-model
   exception: when such a successor exists for either platform, a separate reviewed
   amendment must converge it on the same rule (authenticated dev authority or
   authenticated installed authority; otherwise fail closed).
3. Given a Windows x64 direct package, when the trusted installer installs it, then it
   verifies the signed canonical full artifact, installs that exact artifact as the
   baseline version, protects the immutable version tree, seeds the version floor,
   `current`, and `last-known-good` to that exact baseline, and writes the
   OS-protected provenance commit record last. It records one explicit mode:
   `PerUserDirect`, `MachineUacDirect`, or `MachineSeamlessDirect`. Per-user installs
   live under the installing user's application location and may be mutated by that
   same user; the security claim excludes arbitrary same-user native malware while
   retaining app-role and update-state isolation. Both machine modes install beneath a
   protected machine root readable/executable, but not writable, by ordinary users.
   The mode-specific ACL is read back before the provenance commit record. No record
   admits a package-manager-owned install as direct. The KEL-266 machine-baseline
   initializer and the `PerUserDirect` and `MachineUacDirect` baseline initializers have
   landed as library entrypoints; no shipped installer invokes them yet, and Machine-UAC
   activation and the executable-located selection entrypoint remain incomplete.
4. Given admitted provenance and a normal startup without an activation journal, when
   KEL-53 resolves boot state, then it validates the version floor, `current`, both
   known-good slots, complete markers and package policy. A valid `current` must equal
   `last-known-good` or `previous-known-good`; KEL-53 returns exactly its immutable
   `<update-root>/versions/<version>/tree`. If `current` is invalid but `last-known-good`
   is valid, KEL-53 acquires its recovery ownership, durably republishes `last-known-good`
   and reads it back before returning that exact tree. Missing/invalid LKG, invalid
   floor, mixed identity, unauthorized/orphan pointer, or failed recovery halts before
   app resources. It never guesses the newest directory or silently substitutes the
   install baseline. Given a persisted valid journal and no live attempt owner that
   accepts this exact process as its candidate (KEL-53 criterion 8), KEL-53 takes its
   exclusive attempt lease and proves the prior coordinator/candidate process family has
   exited before recovering the exact journal phase. In a machine mode an ordinary
   process does not recover; it returns KEL-53's typed recovery-required state instead
   (KEL-53 criterion 17).
   `PublishPending`, `AwaitingHealth`, `HealthAccepted`, and `RollbackPending` follow
   their existing KEL-53 phase rules; unknown/live process state, corrupt/mixed journal,
   or failed recovery returns no boot selection. If another coordinator still owns the
   attempt, the new process receives no selection and creates no listener, child, or
   window. Only after durable recovery/readback may KEL-53 return the stable current-tree
   selection or an exact newly authenticated candidate selection.
5. Given a live updater candidate whose connect-back claim the authenticated live
   attempt owner accepted (KEL-53 criterion 8), when KEL-53 selects candidate boot, then
   it verifies the exact journal/current/artifact/owner tuple and returns only that
   candidate's immutable version tree in read-only candidate mode. It does not acquire the updater writer lock, recover an
   orphan, or allow the candidate to self-commit health. Missing, stale, replayed, or
   mismatched candidate evidence refuses before app resources.
6. Given a KEL-53-selected active tree, when `keld-core` derives boot files, then the
   canonical current executable is the literal `keld-host.exe` member of that exact
   `<version>/tree`, and `keld.boot.json` is its literal sibling. The direct install
   root and protected update root remain distinct; the initial baseline is not
   assumed to be the active version after update or rollback. No caller-selected
   descriptor path or arbitrary descendant tree is admitted.
7. Given each direct install mode, when access is checked using its actual owner and
   role tokens, then it satisfies its own cell: `PerUserDirect` grants the installing
   user the expected package/update access but app roles cannot mutate updater state;
   this does not claim isolation from arbitrary native code running as that user.
   `MachineUacDirect` and `MachineSeamlessDirect` grant ordinary users read/execute on
   the selected tree and deny them write/delete/rename/WRITE_DAC/WRITE_OWNER on machine
   package and update state. Every mode checks root ancestors and components for
   reparse/path substitution through the single Windows path owner. The check uses
   effective access for actual tokens; KEL-53 owns and reads back each trusted ACL
   profile. Use existing safe `windows_permissions` descriptor reads and non-mutating
   safe handle-open access probes; this contract adds no production `unsafe`.
8. Given each hostile Windows app-role and webview principal admitted by KEL-53, when
   it attempts to change provenance, install/update files, the floor, journal, `current`,
   either known-good pointer, or helper inputs, then OS access denies the operation.
   Independently alter role-specific and inherited ACEs; the native admission/control
   must detect an unauthorized write grant. Standard-user denial does not stand in for
   this principal-specific proof.
9. Given an active tree that passes provenance, current-selection, and access checks,
   when the existing strict boot parser opens `keld.boot.json`, entry, renderer, and
   `keld.permissions.jsonc`, then it resolves them only under the selected tree through
   the existing Windows path owner, retains the validated entry/permissions handles
   and renderer bytes as the current boot contract requires, rejects reparse escape and
   non-regular/missing targets, and
   keeps the current schema, size, relative-path, and descriptor-digest rules. It does
   not create a second permissions parser or hash/parse the policy bytes a second time;
   KEL-102 remains the one runtime permissions-byte verifier.
10. Given any missing or invalid provenance/current selection, bad root owner/access,
   hostile role grant, reparse/path
   substitution, malformed descriptor, missing/escaping resource, or digest mismatch,
   when the no-flag host starts, then it returns the existing typed boot/identity error
   with actionable repair guidance before listener, child, or window creation. Listener,
   child, and window attempt counters remain zero.
11. Given a trusted signed executable with no protected direct-install record, when it
   starts, then signature success alone does not admit installed mode. Given a protected
   record with a different publisher or app id, when it starts, then it refuses without
   fallback. These are separate negative controls.
12. Given two installed apps with different authenticated KEL-135 app/publisher
   identities under one Windows user, when both open WebView2, then each actual UDF is
   under that user's LocalAppData and differs by the existing `ProfileIdentity`; neither
   UDF is beside the executable or shared. Profile selection consumes the already
   verified identity and cannot make boot-provenance validation pass.
13. Given a managed/package-manager-owned install or a legacy same-user role profile,
    when direct installed-root boot/update admission is requested, then the owner is
    reported as unsupported and no direct mutation or unauthenticated boot fallback is
    attempted.
14. Given an update request, when the install mode is resolved, then `PerUserDirect`
    uses the same user's ordinary authority without UAC; `MachineUacDirect` verifies
    and stages candidate bytes in a separate user-owned location, then obtains explicit
    UAC only for protected activation. Its one-shot OS-authenticated handoff binds the
    initiating user's SID, logon session and caller process. It may identify staged
    candidate files only with KEL-53's bounded lookup locator beneath the authenticated
    user's owner-private staging root; the locator conveys no authority and is not an
    arbitrary caller path. After authenticating the handoff, the elevated updater
    resolves that locator through the Windows path owner, opens and retains the exact
    read-only source handles itself, denies concurrent write/delete where the platform
    permits, and independently revalidates the signed manifest/artifact from those
    handles. It verifies its Authenticode signer and image digest against the protected
    helper identity, creates the protected journal/attempt, copies to a protected
    sibling stage and verifies/read-backs the copy before publication. It accepts no
    mutation authority from argv/environment/cwd, limits writes to that installation's
    package and update roots, and launches the candidate using the exact initiating user's
    ordinary token and logon session, even when UAC used alternate administrator
    credentials. If that token cannot be securely reused, it refuses launch and leaves
    journal-bound recovery. Forged, stale, replayed, wrong-host, cross-install,
    replaced-source and wrong-session requests refuse before mutation. If the elevated
    owner dies, recovery is journal-bound and fail-closed; retry may require a new UAC
    grant. `MachineSeamlessDirect` may mutate only through a Windows-native authority
    selected by a separately approved architecture/spec amendment after KEL-270's
    lifecycle evidence and all remaining named proof gates pass. Until that
    gate passes, it refuses activation before package mutation. In all cells the shared KEL-53
    transaction owns verification, anti-downgrade, journal, health, commit/rollback,
    and recovery; boot admission itself grants no update write authority.
15. Given real signed direct-install fixtures on supported Windows x64, when launched
    by a standard non-elevated user in each admitted direct mode, then the real host
    reaches its expected window and Bun entry for initial, updated, and rolled-back
    active versions; candidate boot reaches only the exact journaled candidate. The host
    reports verified app/profile identity and the actual WebView2 UDF under LocalAppData.
    Evidence binds source head, install mode, package/archive digest, active artifact and
    journal state, Authenticode publisher/app identity, root owner/DACL readback,
    standard-user and role tokens, and resource counters. Machine-UAC activation runs
    only its updater component elevated; the app host and Bun remain ordinary-user.
16. Given independent negative controls for absent/unprotected provenance, wrong
    publisher/app/root/baseline/current artifact, invalid floor/LKG/journal, stale
    candidate claim, orphan/incomplete tree, writable root or ancestor,
    standard-user write/delete/WRITE_DAC, hostile role/webview write, role-specific ACE
    mutation, reparse ancestor, malformed/tampered boot descriptor, missing/escaping
    entry or renderer, and malformed permissions descriptor, when each is attempted,
    then every unrecoverable control refuses before listener/child/window and preserves
    the exact reason plus OS/package evidence. The distinct no-journal control with
    invalid `current` and valid LKG must durably recover to that LKG and may then boot;
    invalid current plus invalid LKG must refuse. Controls that require administrator
    mutation are outside the promise and MUST be labelled as such rather than reported
    as standard-user or role-principal protection evidence.
17. Given the install-mode provenance record, when the host boots or updater requests a
    write lease, then mode cannot be selected by path shape, `%ProgramFiles%`,
    environment, argv, cwd, caller booleans, or ACL observation alone. Independently
    substitute the mode, owner SID, root, updater owner, package owner, and current
    executable; every mismatch refuses before app resources or mutation.
18. Given a package/deployment-owned install, when Keld startup or update admission is
    requested, then Keld does not create direct provenance, select a competing active
    version, or mutate its files. The owner is delegated to the corresponding package
    mechanism; unsupported invocation fails closed with an actionable typed result.
19. Given an explicit-UAC machine update, when the elevated updater receives a request,
    then the native Windows acceptance independently exercises valid one-attempt
    activation, forged request, stale/replayed attempt, wrong host/image, cross-install
    request, outside-root/traversal/reparse source locators, source-handle replacement/
    write races, alternate UAC credentials, wrong SID/logon/helper session, substituted
    or unavailable token/process handles, invalid elevation/integrity level,
    token/process association mismatch, failed impersonation/reversion, unavailable
    desktop/profile, argument substitution and attempted writes outside the exact
    package/update roots. It kills the coordinator/updater at every durable journal
    boundary and proves recovery follows the journal or halts without changing pointers.
    The launched host and Bun tokens match the initiating user/session and remain
    non-elevated.

## 4. Design

### First-principles and reuse decision

| Atom / owner | Boundary and input → output | Failure mode | Independent observable |
|---|---|---|---|
| Mode selection / KEL-53 + `keld-core` | verified dev lease or authenticated install provenance → opaque `DevStage` or mode-tagged direct selection | path/env/caller bool or observed ACL chooses trust mode | independently mutate cwd, environment, argv, executable path, provenance mode, owner SID and root; only the authenticated record controls the cell |
| App identity / KEL-135 | verified current Authenticode image → publisher scope + app id | untrusted, ambiguous or differently signed image is treated as Keld | real signed fixture and wrong-publisher/app negative controls |
| Install provenance / KEL-53 | trusted per-user or machine installer → mode, owner SID, install/update roots and initial baseline identity or refusal | missing/mismatched mode, synthetic record, or managed owner is treated as direct | real installer record/readback per mode plus independently changed record fields |
| Active selection/lifecycle / KEL-53 | provenance + floor + `current` + journal + attempt-owner acceptance → one exact active version tree and normal/candidate mode, with the specified current→LKG recovery | stale baseline, orphan tree, partial update or replayed or substituted candidate claim boots; valid LKG is skipped after a recoverable current failure | independently mutate current/LKG/previous-LKG/floor/journal/marker/path/owner endpoint; only a valid state or the exact approved LKG recovery reaches the boot parser |
| Root containment / Windows install adapter | actual owner/role tokens + recorded mode/root/ancestors → that mode's documented access profile or refusal | owner-mode mismatch, writable machine ancestor, or reparse/replacement permits substitution | effective-token access probes plus owner/DACL/reparse readback for each independent mode cell |
| Update authority / KEL-53 adapter | one common verified attempt + requested write lease + install mode → same-user lease, explicit-UAC lease, proof-gated narrow lease, or managed-owner refusal | a mode-specific authority duplicates transaction policy or silently obtains broader rights | run the same journal/health/rollback trace through each direct authority; compare identical artifact and state transitions; negative controls prove no authority cross-over |
| UAC token/launch / KEL-53 Windows adapter + KEL-96/IPC | authenticated initiating process → exact ordinary-user candidate token/process or refusal | failed impersonation, wrong token, session mismatch, or privilege failure falls back to elevated launch | real Windows standard-user A / alternate-admin B test checks token SID, logon id, session, integrity, elevation, process image, profile and desktop; wrong-process/token/session and failed-impersonation controls refuse |
| Lifecycle retirement / KEL-270 | exact attempt Job + keeper/coordinator state → authenticated process-family-zero proof before releasing the mutation boundary | missing owner, stale/replayed result, wrong Job/host, or ambiguous reboot releases writer/recovery | adversarial death, replay, competing-writer and ambiguous-reboot controls; absence-of-process inference never passes |
| Role containment / KEL-53 security profile | each admitted app-role/webview token + protected state → denied mutation | role-specific ACE grants package/update authority | real role-token write/delete/ACL probes plus role-ACE mutation |
| Boot files / `keld-core` | KEL-53-selected immutable version tree + exact sibling descriptor → validated entry/renderer/permissions handles | sidecar parse/path/digest failure is ignored or files change between validation and use | resource-free rejection and held-handle substitution controls |
| Profile / KEL-135 + WebView2 owner | already verified identity → actual per-app LocalAppData UDF | profile path inferred from install path or identities alias | two real installed identities report distinct actual UDFs |
| Evidence / Windows acceptance owner | exact source/package + native OS observations → acceptance receipt | mocked ACL, stale head, or configured UDF is mistaken for product proof | exact-head signed fixture, real standard-user token and independent negative controls |

Independence edges: Authenticode publisher/app identity is not installation ownership;
installation ownership is not an effective read-only ACL; ACL protection does not prove
the package originated from an approved signer; boot-file parsing does not prove app
identity; profile isolation proves none of the boot predicates. Installed admission is
the conjunction of these predicates. Failure at any predicate stops before resources.

**Reuse:** KEL-135 remains the sole Windows Authenticode signer/app identity extractor
and profile-identity owner. KEL-53 remains the sole direct-install provenance, package
baseline, channel-owner, active-version selection, recovery, candidate/journal and
OS-protection owner. KEL-96 `keld-core` remains the sole no-flag descriptor parser, path
resolver, boot-selection mint and startup-order owner. KEL-102 remains the single
runtime permissions-byte reader/verifier. KEL-270 owns the isolated Windows lifecycle
proof for a possible machine-seamless authority; it does not become a second updater or
transaction writer. Existing Windows component/path and handle-opening primitives
remain the path-resolution owner; their dev-only ACL assumptions must be extended in
that owner for each installed mode.

As of amendment A3, KEL-53 has signed manifest/full-artifact verification, canonical
Windows packaging, protected extraction, the KEL-266 machine-baseline initializer and
loader, PerUserDirect and Machine-UAC baseline provenance, the common journaled
transaction with crash recovery, and the journal-free `ActivePackageSelection`
(`select_windows_active_package`) including the startup repair of an invalid `current`.
It does not yet have the executable-located discovery entrypoint below, candidate-mode
selection, the candidate health channel, Machine-UAC activation, or live feed
orchestration. KEL-53 owns these remaining pieces.
Its admitted result binds the KEL-135 publisher/app identity, explicit install mode and
owner, direct install/update roots, update-signing identity, and initial baseline.
KEL-53 exposes only the authenticated recorded publisher/app identity for `keld-core`
to compare with KEL-135; `keld-update` does not depend on `keld-core`. Its active
selection identifies exactly one current artifact/tree. The baseline is only the
install-time floor; it MUST NOT stand in for the active artifact after update or
rollback. The active resolver validates no-journal recovery state or the exact
candidate journal/endpoint before lending the selected version tree to KEL-96. A
synthetic protected-observation enum is state-machine test evidence only, never proof of
OS record protection, current-pointer authority, installer bytes, or role-token denial.

One KEL-53 transaction owns signed verification, anti-downgrade, staging, journal,
exclusive activation ownership, exact candidate launch, health confirmation,
commit/rollback and crash recovery. Only acquisition of its write lease varies by mode:
same-user for `PerUserDirect`, explicit elevation for `MachineUacDirect`, and an
independently proven narrow authority for `MachineSeamlessDirect`. The seamless mode is
not enabled by selecting a mechanism name. Until KEL-270 proves all approved lifecycle
gates and a separate exact review selects the smallest defensible authority, the
seamless mode refuses activation. Managed/package-manager installs never enter this
direct transaction; mutation remains with their deployment owner.

The installer proves that the protected baseline tree came from its exact authenticated
package before committing provenance. KEL-53 then proves protected active-version
publication/readback and that ordinary users plus every admitted hostile app role and
webview cannot mutate package/update state. The host relies on that admitted selection
and access boundary while it opens the exact boot files. It adds no independent
file-inventory digest, current-pointer parser, journal/recovery owner, or second
permissions hash/parser.

**Rejected alternatives:** removing the dev DACL predicate admits a mutable tree;
Authenticode on the executable says nothing about neighboring sidecars;
`%ProgramFiles%`, path shape, read-only attributes, environment, and cwd are not
installer provenance; a new KEL-96-only registry/receipt would duplicate KEL-53's owner;
an independent per-boot file inventory or second permissions hash/parser would duplicate
package/KEL-102 policy. Raw Win32 `unsafe` access checks are also rejected here because
the current `keld-core/AGENTS.md` owner does not sanction them; use safe descriptor and
non-mutating handle-open access checks. If those existing safe primitives cannot prove
the required predicate, stop and revise the approved contract plus owner instructions
before implementation. Managed installs and absent provenance stay fail-closed.

**Compatibility fallback:** preserve the current dev-stage behavior. Direct installed
boot remains unavailable until the KEL-53 mode-aware producer, active-tree resolver,
and this KEL-96 consumer land with native proof. Per-user direct installs use the
same-user update lease; machine-UAC installs use explicit elevation for activation;
machine-seamless updates remain refused until the KEL-270 proof gate and authority
selection pass. Managed installs continue through their package owner and are not
admitted to Keld's direct updater.

### Selection shape (workspace export)

These are opaque Rust values that `keld-update` exports for `keld-core`, their only Rust
consumer; they are not a wire format, and their export is reviewed under the public-API
gate below. `ExpectedAppIdentity` wraps the canonical build identity whose encoding
`keld-pack` owns through the existing `keld-update -> keld-pack` edge. `keld-pack`
produces those bytes and never imports `keld-update`; `keld build` invokes it, and that
`keld-cli -> keld-pack` edge belongs to the KEL-19 packaging work. The sketch records
ownership only; each Windows adapter continues to return the crate's typed error type.

```rust
enum BootRootMode {
    DevStage,
    InstalledPackage {
        identity: ValidatedAppIdentity,       // KEL-135-owned verified identity
        install_mode: DirectInstallMode,      // KEL-53-owned protected provenance
        active: ActivePackageSelection,       // KEL-53-owned opaque current/candidate tree
    },
}

enum DirectInstallMode {
    PerUserDirect,
    MachineUacDirect,
    MachineSeamlessDirect,
}

struct ValidatedBootSelection {
    mode: BootRootMode,
    app: AppBootSelection, // existing root/entry handle/renderer owner
    permissions_file: File, // existing KEL-102 handoff
    permissions_digest: [u8; 32],
}
```

`ActivePackageSelection` is the opaque value owned by `keld-update`; its journal-free
form has landed. It carries the protected recorded publisher/app identity, install root,
exact active artifact/tree, and whether normal recovery or attempt-bound candidate
admission selected it. Its Rust contract is:

```rust
pub struct DirectInstallationIdentity {
    app_id: String,
    channel: Channel,
    target: String,
    install_mode: DirectInstallMode,
    owner_sid: String,
    install_root: PathBuf,
    update_root: PathBuf,
    signing_key_id: SigningKeyId,
    baseline: ArtifactIdentity,
    publisher_scope: [u8; 32], // exact KEL-135 value; no second hash implementation
    profile_digest: ProfileDigest,
    principal_model: PrincipalModel,
}

impl DirectInstallationIdentity {
    /// Returns the canonical app id stored in protected install provenance.
    pub fn app_id(&self) -> &str;
    /// Returns the exact publisher-scope value produced by KEL-135.
    pub fn publisher_scope(&self) -> &[u8; 32]; // exact KEL-135 identity value
}

pub struct ActivePackageSelection { /* private fields; not Clone */ }

/// Non-secret build-time expectation: app id, channel, target and the expected
/// update-signing key identity. Produced only by `keld-pack`/`keld build` inside the
/// bytes covered by the executable's final signature; it never contains private key
/// material.
pub struct ExpectedAppIdentity { /* private fields */ }

impl ExpectedAppIdentity {
    /// Decodes the embedded build identity through `keld-pack`'s canonical decoder.
    pub fn decode(embedded: &[u8]) -> Result<Self, UpdateError>;
}

pub fn select_active_package_for_executable(
    executable: &Path, // locator only: the canonical current executable
    expected: &ExpectedAppIdentity,
) -> Result<ActivePackageSelection, UpdateError>;

impl ActivePackageSelection {
    pub fn install_identity(&self) -> &DirectInstallationIdentity;
    pub fn artifact(&self) -> &ArtifactIdentity;
    pub fn tree_root(&self) -> &Path;
    pub fn launch_kind(&self) -> ActiveLaunchKind;
}

pub enum ActiveLaunchKind { Normal, Candidate }
```

Only the KEL-53 OS loader/state machine can mint it. **The executable path locates
candidate provenance; authenticated provenance supplies authority.** From the canonical
current executable KEL-53 derives candidate locations by one fixed rule, then proves
them. The executable's file name must be literally `keld-host.exe`; its parent is the
candidate tree and must be named `tree`; the tree's parent is the version directory and
must have a strict-SemVer name; that directory's parent must be named `versions`; the
parent of `versions` is the candidate update root, and the update root's parent is the
candidate install root. The protected record is the install root's literal
`install-provenance` entry, and the install root must contain exactly that record and
the update-root directory, as the landed loader already requires. Any other shape
refuses before resources. The protected provenance record must name roots that are the
located roots by
file identity (volume serial number and file index, never path text); its volume GUID
must read back; every ancestor and record must carry the exact protection profile of the
*recorded* install mode; and the selected tree's literal `keld-host.exe` must be the
running executable by file identity. No install mode, owner SID, root, volume, baseline,
current selection, protection profile or profile digest, and no trust decision, comes
from path shape, `%ProgramFiles%`, registry, environment, cwd, argv or an ACL observation
alone; the protected record stays authoritative for each. `ExpectedAppIdentity` carries
only non-secret expectations from one canonical build-time producer; runtime code never
hand-writes them. `keld-pack` embeds their canonical encoding exactly once in
`keld-host.exe` before the executable's final Authenticode signature, and T2b fixes the
container. At boot `keld-core` reads those bytes from the running executable only after
KEL-135 has verified that executable's signature, and constructs the value with
`ExpectedAppIdentity::decode`; a missing, duplicated or malformed encoding refuses before
resources. KEL-53 refuses unless the record matches that expectation, and KEL-96
independently requires the record's publisher scope and app id to equal the KEL-135
Authenticode identity of the same executable. The entrypoint reads no environment
payload or argv for authority, and candidate admission comes from protected attempt
state (KEL-53 criterion 8). It validates provenance, current/LKG/floor, a
journal phase (including process-family ownership/death and durable recovery when
needed), and, for a candidate claimant, the live owner's acceptance. If another live
coordinator owns the journal, process state is unknown, or recovery is incomplete,
KEL-53 returns no selection. KEL-96 compares the record's publisher/app fields with the KEL-135 verified
identity, checks the effective access boundary, and verifies that `current_exe` is the
exact host inside `tree_root`. It may consume but MUST NOT construct or clone the
selection. `DirectInstallationIdentity` and its OS-protected record are KEL-53
deliverables; the record's format stays OS-local. No code may construct installed mode
from paths or test observations.
`ValidatedBootSelection` remains opaque and is the only selection accepted by
`run_unprivileged` / `run_guarded`.

Capabilities required; manifest changes: none.

Wire/protocol changes: none; KEL-53 feed bytes remain unchanged. The KEL-53 protected
installer record is OS-local state, not a renderer or KIPC wire contract.

Platform notes: Windows x64 direct install is the only installed-root cell in this
spec. Its independently qualified cells are per-user direct, machine-wide with
explicit-UAC activation, and machine-wide seamless activation only after the KEL-270
authority proof gate. Dev-stage behavior on all currently proved platforms remains
governed by KEL-96. macOS app-container/signing and Linux package/root admission require
their own approved successors and real OS qualification. Managed Windows installs
remain owned by their package/deployment mechanism and never receive a competing Keld
writer.

Runtime seam: `keld-host` remains thin and calls the existing `keld-core` boot-selection
entrypoint. Within the host process, `keld-core` derives `current_exe`; KEL-135 verifies
the image and returns immutable publisher/app identity;
KEL-53 validates installer provenance and resolves normal or candidate state into one
opaque active-tree selection; KEL-96 compares the identities and executable path; the
Windows root owner checks effective user/role access and opens the selected tree; then
the existing boot parser validates the descriptor and targets. Only after guard
preflight may the host create listener, child, or window. Any failed step returns a
typed error before resources. The same verified identity remains fixed through profile
creation and app-session teardown. The candidate health endpoint remains owned by
KEL-53 and does not become app authority.

Migration unit: `keld-host` remains the thin caller of the existing core boot entrypoint;
`keld-core` consumes a KEL-53 active-tree selection and its `ValidatedBootSelection`
gains an opaque installed variant. The KEL-53
provenance/current-selection interfaces are authoritative and cannot be duplicated in
the host; they stay until direct installed boot and update admission no longer depend on
them. No other app caller, generated contract, persisted user data, permission grant,
or KIPC session changes.

## 5. Boundaries

Implement in:

- `crates/keld-core/Cargo.toml` and `crates/keld-core/src/app_session.rs`: add only
  the internal `keld-update` dependency and consume its landed opaque active-selection
  API using KEL-135 identity and the Windows root-opening owner;
- **KEL-53 predecessor deliverables:** extend the landed Windows machine-baseline
  initializer/loader and package/extraction path with mode-aware installation
  provenance, per-user initialization, active current/LKG selection and the common
  journal/health/rollback/recovery state machine; keep the per-user, machine-UAC and
  machine-seamless write-lease adapters distinct and managed-owner mutation delegated;
- the Windows native host acceptance harness: add signed fixtures and per-mode
  standard-user/owner/role-token evidence after the KEL-53 predecessor lands.

Must not touch:

- the KEL-96 owner-private dev-stage DACL policy except to preserve it unchanged;
- `crates/keld-guard` policy, generated permissions, KIPC frame/wire contract, or
  renderer authority;
- KEL-135 signer/profile hashing or WebView2 storage policy; reuse its verified value;
- the update-feed wire schema, delta selection, or package-manager mutation APIs;
- root/nested AGENTS, global workspace dependencies, or unrelated OS boot paths.

The KEL-96 consumer MUST NOT copy or alter the KEL-53 updater recovery state machine.
The KEL-53 predecessor implements and qualifies that state machine under its own issue;
it is a required dependency of this consumer.

The consumer adds one internal workspace-crate edge, `keld-core -> keld-update`, to pass
the opaque selection directly to the boot owner. This adds no third-party dependency or
workspace member. `keld-update` MUST NOT depend on `keld-core`; Cargo metadata and the
workspace build prove the graph remains acyclic. The selection and identity types stay
opaque outside their owner except for the documented read-only identity accessors.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [x] T1 — exact-content approval recorded; synchronize
  architecture 03/06 plus the KEL-96 D4 applicability note in the same spec PR. Keep
  all KEL-96 dev-stage behavior and the frozen KEL-96 decision digest unchanged. Product
  acceptance and implementation remain tracked in later tasks.
- [ ] T2 — in the KEL-53 owner issue, implement mode-aware provenance and active
  selection, per-user initialization, shared baseline/current/LKG/journal/candidate
  validation, immutable version-tree and mode-specific ACL readback, and a typed opaque
  active-package selection. Prove each mode's owner/role access cell. Keep one shared
  updater state machine; connect only the per-user and explicit-UAC lease adapters after
  their own acceptance. Machine-seamless activation remains gated on KEL-270. This is a
  strict predecessor; KEL-254 does not copy KEL-53 recovery or pointer policy.
- [ ] T2b — make `keld-pack`/`keld build` the one canonical producer of
  `ExpectedAppIdentity` (app id, channel, target, expected update-signing key identity),
  embedded in the bytes covered by the final executable signature; no runtime code or
  test hand-writes these values. KEL-53 adds `select_active_package_for_executable`,
  which locates provenance from the executable and proves it as §4 requires.
- [ ] T3 — in the KEL-96 consumer issue, consume the landed KEL-53 active selection and
  one KEL-135 identity value to add opaque Windows installed-root boot. Verify the
  executable is the exact selected version-tree host, preserve current strict parser and
  resource-free ordering, and pass the same verified identity to profile selection.
  Enforce the two Windows boot states of AC2 and move the lease-less signed dev-stage
  KEL-135 rows to a real dev lease or to installed fixtures; no test keeps a third state.
- [ ] T4 — on a real Windows x64 machine, install and launch signed initial, updated,
  rolled-back and candidate fixtures as a standard user; run independent provenance,
  pointer/journal, standard-user ACL, role-token, reparse, descriptor, resource and
  profile controls; retain exact-head owner/DACL/package/UDF/resource evidence. Close
  only after every required native row passes.

## 7. Test plan

| Criteria | Proof and falsifier |
|---|---|
| 1 | Existing dev-stage regression on Windows; mutate release identity source to panic and prove valid dev lease still bypasses release identity. |
| 2–3, 11, 17–18 | State tests reject absent/managed/unprotected/corrupt/mismatched provenance; real installer crash cuts before/after final commit prove that wrong package digest, missing file, wrong mode/owner, failed mode-specific ACL readback or record replay leaves no admission. |
| 4, 6, 15–16 | Independently permute initial baseline, current, LKG, previous-LKG, floor, complete marker, current tree, and wrong app id; valid current must equal an allowed known-good artifact. Invalid current with valid LKG durably republishes/read-backs LKG, then boots that version; invalid LKG, orphan/incomplete/mixed versions halt. Inject crashes at each KEL-53 journal phase with and without a live attempt owner: recovery must prove the process family/lease, finish the exact phase or return no selection, and never let KEL-96 boot a stale tree. Valid updated and explicit rollback versions boot. |
| 5, 15–16 | Candidate tests substitute owner endpoint, owner identity, launched process, attempt id, journal phase, current artifact and executable path independently; only the exact live attempt selects candidate mode, without lock/recovery/self-commit. |
| 6–10, 16 | Native Windows component/reparse/path tests plus strict descriptor mutations; under each admitted token, attempt descriptor/entry/renderer replacement and prove the protected namespace denies it; malformed bytes and escaping/missing targets fail pre-resource; KEL-102 proves one exact manifest read. |
| 7–8, 16–18 | Per-user owner, ordinary machine user, explicit-UAC activator and each admitted hostile role/webview token exercise distinct access cells. Machine standard-user probes attempt create/write/delete/rename/WRITE_DAC/WRITE_OWNER at roots, versions, provenance, journal, pointers and update state. Per-user tests verify owner write access while hostile roles cannot mutate updater state; they explicitly do not claim defense from same-user native malware. Mutate owner, inherited/explicit ACE, role ACE and reparse ancestor; every forbidden grant is detected before boot. |
| 12, 15 | Signed Windows fixture for two distinct app identities; capture WebView2-reported UDFs and prove each is the exact LocalAppData profile path and they differ. |
| 13–14, 18–19 | Managed-owner fixtures prove no direct provenance, competing selector or writer. Mode-matched tests prove per-user no-UAC update, explicit-UAC machine activation and pre-gate seamless-mode refusal without mutation. Machine-UAC tests prove the only source locator resolves beneath the authenticated owner's private staging root; absolute/outside-root/traversal/reparse/cross-install locators and changed or concurrently writable source objects refuse before protected publication. |

Anti-flake: use named fixtures and fresh per-run install roots; no sleep-sync. Process,
window, file-access, installer commit, and resource-order witnesses use explicit handles,
acknowledgements, or bounded event waits. Real Windows acceptance is not inferred from
CI or a synthetic `Protected` value.

## 8. Review gates triggered

- unsafe: none in this contract; production implementation uses safe existing wrappers
  and non-mutating access probes. A later need for production unsafe blocks this spec
  until an issue-scoped owner-instruction amendment is approved and the exact unsafe
  diff receives independent review;
- public API: yes, if the KEL-53 provenance identity fields are added to its exported
  Rust type; yes unconditionally for the proposed exported `ActivePackageSelection`,
  `ActiveLaunchKind`, `ExpectedAppIdentity` and `select_active_package_for_executable`
  contract. Independent exact-diff API review is required;
- permission model: yes — the boot admission boundary depends on effective filesystem
  rights and protected provenance;
- dependency addition: yes — one internal workspace edge `keld-core -> keld-update`,
  with no third-party crate and no reverse edge; independent review verifies ownership
  and an acyclic Cargo graph;
- wire protocol: none.

## 9. Perf impact

No speedup is claimed. Installed boot adds executable identity verification, protected
provenance admission, effective root access validation and the existing descriptor
preflight. Measure cold and warm host-to-window latency on the exact Windows fixture
before setting any new budget. Do not hash the whole installed tree at every launch;
the installer verifies the exact package and read-backs the protected tree before its
final provenance commit, while the host validates the OS write boundary and the exact
boot files it consumes.

## 10. Open questions

No unresolved product choice remains for the three direct modes or managed-owner
delegation. This spec's exact content was approved by Linear comment
`e0b276f9-5ecc-42a9-ac8f-8a5e05f44245`. The UAC and per-mode native acceptance cells
remain implementation gates and are not claimed passed here.

The machine-seamless authority is an explicit proof gate, not an open invitation to
choose a convenient mechanism. KEL-270 must first prove exact attempt identity, fresh
attempt binding, replay resistance, installation-wide writer exclusion, process-family
lifecycle/death, ordinary-user candidate launch, exact health binding, crash/reboot
recovery, and wrong-host/wrong-role/fake-endpoint negative controls. Ambiguous reboot
or all-owners-lost recovery remains fail-closed. Only then may the smallest defensible
Windows authority be selected; until that evidence exists, `MachineSeamlessDirect`
refuses activation without mutating package state.
