# Spec: authenticated Windows installed-root boot
Status: draft
Linear: KEL-254 · Owner: GYLDLAB · Updated: 2026-09-25

## 1. Goal & non-goals

Allow a standard Windows user to start a directly installed Keld app from a protected
install root after the host proves that the executable, app identity, installer
provenance, and boot files all belong to the installed package. Keep the current
owner-private dev-stage path unchanged and fail closed before creating a listener,
child, or window when any installed-package fact is absent or mismatched.

Non-goals:

- no weakening or removal of the existing Windows dev-stage DACL;
- no mode selection from `%ProgramFiles%`, executable location, environment, cwd,
  command-line data, a read-only bit, or a caller-supplied release flag;
- no claim that Authenticode on `keld-host.exe` authenticates neighboring files;
- no resistance claim against administrators or arbitrary same-user native malware;
- no admission of package-manager/store-owned installs, macOS/Linux packages, or legacy
  same-user role profiles;
- no claim that an ordinary booting user can write or update a machine-protected install;
  KEL-53 must supply a separately authorized updater writer, otherwise update requests
  remain refused;
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
  protected provenance, baseline/current/LKG package identity, candidate journal and
  endpoint, update owner refusal, and real Windows evidence. This is an approved future
  contract; the current repository still has only the `keld-update::Channel` skeleton.
- `docs/specs/kel135-persistent-profile-identity.md` §§3, 4, 7: current Windows
  Authenticode identity, publisher/app separation, and identity-derived LocalAppData
  WebView2 profile.
- `docs/specs/kel102-host-guard-enforcement.md`: one-read permissions verifier and
  exact bytes.

This successor does not change the four-unique architecture or add a trust principal.
The approved implementation PR MUST update architecture 03/06 and KEL-96 D4 in the
same change so the documented release boundary matches the code.

## 3. Acceptance criteria (binary, each becomes a test)

1. Given a valid authenticated private dev lease and the current owner-private stage,
   when the host boots, it follows the existing `DevStage` validation path byte-for-byte
   in behavior. The installed-root path is not consulted and the dev DACL predicate is
   not weakened.
2. Given no valid dev lease, when the Windows host considers installed mode, then it
   obtains the KEL-135 `ValidatedAppIdentity` from the current executable's
   single-primary Authenticode verification and asks the KEL-53 owner to validate
   OS-protected direct-install provenance. The repository has no landed KEL-53
   provenance type or OS loader yet; the installed path remains unavailable until that
   predecessor is implemented and qualified. Missing, managed, corrupt, unprotected,
   or mismatched provenance is a typed refusal; signature success alone never admits
   installed mode and it never falls through to dev-stage or source-config boot.
3. Given a Windows x64 direct package, when the trusted installer installs it, then it
   verifies the signed canonical full artifact, installs that exact artifact as the
   baseline version, protects the immutable version tree, seeds the version floor,
   `current`, and `last-known-good` to that exact baseline, and writes the
   OS-protected provenance commit record last. A failure before the final record leaves
   no boot-admitting provenance. This is a KEL-53 predecessor; it is not an adapter
   already present in `origin/main` and this spec adds no parallel store.
4. Given admitted provenance and a normal startup without an activation journal, when
   KEL-53 resolves boot state, then it validates the version floor, `current`, both
   known-good slots, complete markers and package policy. A valid `current` must equal
   `last-known-good` or `previous-known-good`; KEL-53 returns exactly its immutable
   `<update-root>/versions/<version>/tree`. If `current` is invalid but `last-known-good`
   is valid, KEL-53 acquires its recovery ownership, durably republishes `last-known-good`
   and reads it back before returning that exact tree. Missing/invalid LKG, invalid
   floor, mixed identity, unauthorized/orphan pointer, or failed recovery halts before
   app resources. It never guesses the newest directory or silently substitutes the
   install baseline. Given a persisted valid journal and no authenticated live candidate
   endpoint, KEL-53 takes its exclusive attempt lease and proves the prior coordinator/
   candidate process family has exited before recovering the exact journal phase.
   `PublishPending`, `AwaitingHealth`, `HealthAccepted`, and `RollbackPending` follow
   their existing KEL-53 phase rules; unknown/live process state, corrupt/mixed journal,
   or failed recovery returns no boot selection. If another coordinator still owns the
   attempt, the new process receives no selection and creates no listener, child, or
   window. Only after durable recovery/readback may KEL-53 return the stable current-tree
   selection or an exact newly authenticated candidate selection.
5. Given a live updater candidate and the authenticated inherited attempt endpoint,
   when KEL-53 selects candidate boot, then it verifies the exact journal/current/
   artifact/endpoint tuple and returns only that candidate's immutable version tree in
   read-only candidate mode. It does not acquire the updater writer lock, recover an
   orphan, or allow the candidate to self-commit health. Missing, stale, replayed, or
   mismatched candidate evidence refuses before app resources.
6. Given a KEL-53-selected active tree, when `keld-core` derives boot files, then the
   canonical current executable is the literal `keld-host.exe` member of that exact
   `<version>/tree`, and `keld.boot.json` is its literal sibling. The direct install
   root and protected update root remain distinct; the initial baseline is not
   assumed to be the active version after update or rollback. No caller-selected
   descriptor path or arbitrary descendant tree is admitted.
7. Given the real standard-user token, when access is checked, then the user can read
   and execute the selected package but cannot create, write, delete, rename, change
   owner, or change the DACL of the version tree, package files, provenance, floor,
   journal, pointers, or helper inputs. Every root ancestor and component needed to
   resolve these locations is checked for reparse/path substitution by the single
   Windows install-path owner. The check uses effective object access for the actual
   token; the KEL-53 producer owns and reads back the trusted install ACL profile. The
   production check uses existing safe `windows_permissions` descriptor reads and
   non-mutating safe handle-open access probes; this contract adds no production
   `unsafe`.
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
14. Given a standard-user update request against the protected machine install before
    KEL-53 supplies a separately authorized writer, when the update is attempted, then
    it refuses with a typed owner/access error before modifying the package tree. Boot
    admission does not grant update write authority.
15. Given the real signed direct-install fixture on supported Windows x64, when launched
    by a standard non-elevated user from its protected install root, then the real host
    reaches its expected window and Bun entry for initial, updated, and rolled-back
    active versions; candidate boot reaches only the exact journaled candidate. The host
    reports verified app/profile identity and the actual WebView2 UDF under LocalAppData.
    Evidence binds source head, package/archive digest, active artifact and journal state,
    Authenticode publisher/app identity, root owner/DACL readback, standard-user and role
    tokens, and resource counters.
16. Given independent negative controls for absent/unprotected provenance, wrong
    publisher/app/root/baseline/current artifact, invalid floor/LKG/journal, stale
    candidate endpoint, orphan/incomplete tree, writable root or ancestor,
    standard-user write/delete/WRITE_DAC, hostile role/webview write, role-specific ACE
    mutation, reparse ancestor, malformed/tampered boot descriptor, missing/escaping
    entry or renderer, and malformed permissions descriptor, when each is attempted,
    then every unrecoverable control refuses before listener/child/window and preserves
    the exact reason plus OS/package evidence. The distinct no-journal control with
    invalid `current` and valid LKG must durably recover to that LKG and may then boot;
    invalid current plus invalid LKG must refuse. Controls that require administrator
    mutation are outside the promise and MUST be labelled as such rather than reported
    as standard-user or role-principal protection evidence.

## 4. Design

### First-principles and reuse decision

| Atom / owner | Boundary and input → output | Failure mode | Independent observable |
|---|---|---|---|
| Mode selection / `keld-core` | current executable + validated dev lease or installed evidence → opaque `DevStage` or `InstalledPackage` selection | path/env/caller bool chooses trust mode | mutate cwd, environment, argv and executable location independently; mode does not change without its owning proof |
| App identity / KEL-135 | verified current Authenticode image → publisher scope + app id | untrusted, ambiguous or differently signed image is treated as Keld | real signed fixture and wrong-publisher/app negative controls |
| Install provenance / KEL-53 | installer-created OS-protected record → direct owner, install/update roots and initial baseline identity or refusal | synthetic/caller-created/unprotected/mismatched record admits boot | real Windows record producer/readback plus independently changed record fields |
| Active selection/lifecycle / KEL-53 | provenance + floor + `current` + journal/attempt endpoint → one exact active version tree and normal/candidate mode, with the specified current→LKG recovery | stale baseline, orphan tree, partial update or replayed candidate endpoint boots; valid LKG is skipped after a recoverable current failure | independently mutate current/LKG/previous-LKG/floor/journal/marker/path/endpoint; only a valid state or the exact approved LKG recovery reaches the boot parser |
| Root containment / Windows install adapter | standard token + recorded root/ancestors → read/execute-only package tree or refusal | writable ancestor/reparse/replacement permits substitution | effective-token access probes plus owner/DACL/reparse readback |
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
runtime permissions-byte reader/verifier. Existing Windows component/path and
handle-opening primitives remain the path-resolution owner; their dev-only ACL
assumptions must be extended in that owner for the installed tree.

No `DirectInstallationIdentity`, `ProvenanceObservation`, protected-record loader,
installer, or active-tree selection API has landed in `origin/main`; the workspace's
`keld-update` crate currently implements only `Channel`. KEL-53 must first implement and
qualify one OS-protected record/selection owner. Its admitted result must bind the
KEL-135 publisher/app identity, direct install/update roots, update-signing identity and
initial baseline. KEL-53 exposes the authenticated recorded publisher scope/app id for
`keld-core` to compare with its KEL-135 verified value; `keld-update` does not depend on
`keld-core`. Its active-selection result identifies exactly one current artifact/tree.
The baseline is only the install-time floor; it MUST NOT stand in for the active
artifact after update or rollback. The active resolver validates no-journal recovery
state or the exact candidate journal/endpoint before it lends the selected version tree
to KEL-96. A synthetic protected-observation enum is state-machine test evidence only,
never proof of OS record protection, current-pointer authority, installer bytes, or
role-token denial.

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
boot remains unavailable until the KEL-53 producer, active-tree resolver, and this
KEL-96 consumer both land with native proof. Managed installs continue to use their own
package owner and are not admitted by this contract. A booting standard user receives
no update write authority; KEL-53 must prove a separate narrow writer or refuse the
update request before mutation.

### Internal selection shape

These are internal opaque values, not a public API or wire format. The sketch records
ownership only; each Windows adapter continues to return the crate's typed error type.

```rust
enum BootRootMode {
    DevStage,
    InstalledPackage {
        identity: ValidatedAppIdentity,       // KEL-135-owned verified identity
        active: ActivePackageSelection,       // KEL-53-owned opaque current/candidate tree
    },
}

struct ValidatedBootSelection {
    mode: BootRootMode,
    app: AppBootSelection, // existing root/entry handle/renderer owner
    permissions_file: File, // existing KEL-102 handoff
    permissions_digest: [u8; 32],
}
```

`ActivePackageSelection` is a required future opaque value owned by `keld-update`, not a
type currently present in the workspace. It carries the protected recorded publisher/app
identity, install root, exact active artifact/tree, and whether normal recovery or
attempt-bound candidate admission selected it. Its proposed Rust contract is:

```rust
pub struct DirectInstallationIdentity {
    app_id: String,
    channel: Channel,
    target: String,
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

pub fn select_active_package_for_current_process(
) -> Result<ActivePackageSelection, UpdateError>;

impl ActivePackageSelection {
    pub fn install_identity(&self) -> &DirectInstallationIdentity;
    pub fn artifact(&self) -> &ArtifactIdentity;
    pub fn tree_root(&self) -> &Path;
    pub fn launch_kind(&self) -> ActiveLaunchKind;
}

pub enum ActiveLaunchKind { Normal, Candidate }
```

Only the KEL-53 OS loader/state machine can mint it. The no-argument entrypoint reads no
caller path, environment payload, or argv for authority; KEL-53 owns its protected-state
and inherited-candidate-handle discovery. It validates provenance, current/LKG/floor, a
journal phase (including process-family ownership/death and durable recovery when
needed), and the candidate endpoint when present. If another live coordinator owns the
journal, process state is unknown, or recovery is incomplete, KEL-53 returns no
selection. KEL-96 compares the record's publisher/app fields with the KEL-135 verified
identity, checks the effective access boundary, and verifies that `current_exe` is the
exact host inside `tree_root`. It may consume but MUST NOT construct or clone the
selection. `DirectInstallationIdentity` and its OS-protected record are also new KEL-53
deliverables, not existing Rust items; the record's format stays OS-local. No code may
construct installed mode from paths or test observations.
`ValidatedBootSelection` remains opaque and is the only selection accepted by
`run_unprivileged` / `run_guarded`.

Capabilities required; manifest changes: none.

Wire/protocol changes: none; KEL-53 feed bytes remain unchanged. The KEL-53 protected
installer record is OS-local state, not a renderer or KIPC wire contract.

Platform notes: Windows x64 direct install is the only installed-root cell in this
spec. Dev-stage behavior on all currently proved platforms remains governed by KEL-96.
macOS app-container/signing and Linux package/root admission require their own approved
successors and real OS qualification. Managed Windows installs remain refused here.

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
- **KEL-53 predecessor deliverables (not landed):** implement its protected
  provenance/installer/platform adapter in `crates/keld-update` and its Windows x64
  artifact path in `crates/keld-pack`; these crates currently contain only the channel
  and format skeletons. The KEL-53 issue owns the real producer/loader/current-state and
  package-to-protected-tree proof;
- the Windows native host acceptance harness: add the signed installed fixture and
  standard-user/role-token evidence after the KEL-53 predecessor lands.

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

- [ ] T1 — approve this contract and synchronize architecture 03/06 plus KEL-96 D4 in
  the same spec PR. Keep all KEL-96 dev-stage behavior unchanged.
- [ ] T2 — in the KEL-53 owner issue, implement the Windows protected installation
  record, baseline installer, active `current`/LKG resolver, journal/candidate
  validation, immutable version-tree and ACL readback, and a typed opaque active-package
  selection. Prove standard-user and each admitted hostile-role/webview token cannot
  mutate package or update state. Define the narrow update writer; if none is available,
  update calls against this machine install refuse before mutation. This is a strict
  predecessor; KEL-254 does not copy KEL-53 recovery or pointer policy.
- [ ] T3 — in the KEL-96 consumer issue, consume the landed KEL-53 active selection and
  one KEL-135 identity value to add opaque Windows installed-root boot. Verify the
  executable is the exact selected version-tree host, preserve current strict parser and
  resource-free ordering, and pass the same verified identity to profile selection.
- [ ] T4 — on a real Windows x64 machine, install and launch signed initial, updated,
  rolled-back and candidate fixtures as a standard user; run independent provenance,
  pointer/journal, standard-user ACL, role-token, reparse, descriptor, resource and
  profile controls; retain exact-head owner/DACL/package/UDF/resource evidence. Close
  only after every required native row passes.

## 7. Test plan

| Criteria | Proof and falsifier |
|---|---|
| 1 | Existing dev-stage regression on Windows; mutate release identity source to panic and prove valid dev lease still bypasses release identity. |
| 2–3, 11 | State tests reject absent/managed/unprotected/corrupt/mismatched provenance; real installer crash cuts before/after final commit prove that wrong package digest, missing file, writable root, failed ACL readback or record replay leaves no admission. |
| 4, 6, 15–16 | Independently permute initial baseline, current, LKG, previous-LKG, floor, complete marker, current tree, and wrong app id; valid current must equal an allowed known-good artifact. Invalid current with valid LKG durably republishes/read-backs LKG, then boots that version; invalid LKG, orphan/incomplete/mixed versions halt. Inject crashes at each KEL-53 journal phase with and without a live candidate endpoint: recovery must prove the process family/lease, finish the exact phase or return no selection, and never let KEL-96 boot a stale tree. Valid updated and explicit rollback versions boot. |
| 5, 15–16 | Candidate tests substitute endpoint, attempt id, journal phase, current artifact and executable path independently; only the exact live attempt selects candidate mode, without lock/recovery/self-commit. |
| 6–10, 16 | Native Windows component/reparse/path tests plus strict descriptor mutations; under each admitted token, attempt descriptor/entry/renderer replacement and prove the protected namespace denies it; malformed bytes and escaping/missing targets fail pre-resource; KEL-102 proves one exact manifest read. |
| 7–8, 16 | Real standard-user token attempts create/write/delete/rename/WRITE_DAC/WRITE_OWNER at root, ancestors, every version tree, provenance, journal, pointers and update state. Repeat under each admitted hostile role/webview token. Mutate owner, inherited ACE, explicit ACE, role-specific ACE, and reparse ancestor; every unauthorized grant is detected before boot. Positive control confirms read/execute. |
| 12, 15 | Signed Windows fixture for two distinct app identities; capture WebView2-reported UDFs and prove each is the exact LocalAppData profile path and they differ. |
| 13–14 | Managed-owner and legacy same-user profile fixtures refuse direct admission; an ordinary user cannot update a machine root without the KEL-53 writer and the attempt leaves package state unchanged. |

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
  Rust type; yes unconditionally for the proposed exported `ActivePackageSelection` and
  `ActiveLaunchKind` contract. Independent exact-diff API review is required;
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

Human approval required before implementation: approve KEL-53 as the single owner of
protected provenance plus current-version/candidate selection, with KEL-135's existing
publisher/app value bound to that record and passed through to KEL-96. This preserves one
package source of truth and avoids a second current-pointer or journal parser.

Also approve the explicit scope boundary for machine installs: boot can be read/execute
only for the standard user; update writes remain refused until KEL-53 proves a separate
narrow updater writer. Recommendation: keep those authorities separate and do not grant
the booting host package-write access. If either owner cannot prove its side of this
contract, keep installed boot rejected and revise the spec before code.
