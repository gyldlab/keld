# Spec: source-qualified Electron-to-Keld bridge release recipe
Status: draft
Linear: GH-583 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; docs only, do not merge.

## 1. Goal & non-goals

Define a future reviewable release recipe in which a final Electron release can direct its already-installed electron-updater
client to an approved Keld installation artifact. Reachability is established per installed client, target, signing identity and
installation mode. Record blocked rows and exact next evidence instead of claiming that a produced artifact reaches existing
users. Current user authorization covers this documentation; implementation and feed publication stay parked.

Non-goals: building/installing an artifact, publishing or redirecting a live feed, operating a release server, changing an existing user's
installation, implementing Keld installers, giving Keld runtime code an installer/package-manager spawn path, qualifying a
platform from a different platform's success, bypassing KEL-53/KEL-254 or public-distribution approval.

## 2. Spec refs

- [GH-583 platform decisions and parked status](https://github.com/gyldlab/keld/issues/583)
- [Compatibility architecture bridge direction](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/04-electron-compat.md)
- [Current distribution/runtime and lifecycle limits](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/06-runtime-and-tooling.md)
- [KEL-53 admitted activation and mode proof](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/kel53-full-package-activation.md)
- [Current production PerUserDirect-only transaction](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-update/src/lib.rs)
- [Windows installed root and launch identity](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/kel254-windows-installed-root-boot.md)
- [Independent local macOS packaging proposal](https://github.com/gyldlab/keld/blob/1e7a32db38e5f9b43614407e7b55086f46f5bdb9/docs/specs/macos-local-app-packaging.md)
- [draw.io Windows x64 config](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/electron-builder-win.json)
- [draw.io arm64/portable config](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/electron-builder-win-arm64.json)
- [draw.io Linux/macOS config](https://github.com/jgraph/drawio-desktop/blob/2edf9fb97eff9bd94bdda33abfe1ee9e7f7bb55e/electron-builder-linux-mac.json)
- [electron-updater6.8.9 NSIS client](https://github.com/electron-userland/electron-builder/blob/bed3a9c421a8a05c49f36a2af5d7ae3598cb1b95/packages/electron-updater/src/NsisUpdater.ts)
- [electron-updater6.8.9 Windows signature verifier](https://github.com/electron-userland/electron-builder/blob/bed3a9c421a8a05c49f36a2af5d7ae3598cb1b95/packages/electron-updater/src/windowsExecutableCodeSignatureVerifier.ts)
- [electron-updater6.8.9 Mac client](https://github.com/electron-userland/electron-builder/blob/bed3a9c421a8a05c49f36a2af5d7ae3598cb1b95/packages/electron-updater/src/MacUpdater.ts)
- [electron-updater6.8.9 AppImage client](https://github.com/electron-userland/electron-builder/blob/bed3a9c421a8a05c49f36a2af5d7ae3598cb1b95/packages/electron-updater/src/AppImageUpdater.ts)
- [electron-updater6.8.9 Linux privileged command boundary](https://github.com/electron-userland/electron-builder/blob/bed3a9c421a8a05c49f36a2af5d7ae3598cb1b95/packages/electron-updater/src/LinuxUpdater.ts)
- [electron-updater6.8.9 Debian client](https://github.com/electron-userland/electron-builder/blob/bed3a9c421a8a05c49f36a2af5d7ae3598cb1b95/packages/electron-updater/src/DebUpdater.ts)
- [electron-updater6.8.9 RPM client](https://github.com/electron-userland/electron-builder/blob/bed3a9c421a8a05c49f36a2af5d7ae3598cb1b95/packages/electron-updater/src/RpmUpdater.ts)
- [electron-updater6.8.9 Pacman client](https://github.com/electron-userland/electron-builder/blob/bed3a9c421a8a05c49f36a2af5d7ae3598cb1b95/packages/electron-updater/src/PacmanUpdater.ts)

Verified source and limits: The pinned draw.io Windows configs request perMachine NSIS and publisherName [JGraph Ltd, draw.io Ltd]; arm64 uses latest-arm64
and includes portable. Source config is not an inspected installed app-update.yml or published latest.yml. NsisUpdater6.8.9
checks publisherName only when present in configOnDisk and constructs --updated, optional /S, --force-run and /D arguments. It
consults downloaded isAdminRightsRequired and has explicit elevation fallback. AppImage replaces the old file; Debian/RPM/Pacman
delegate installation commands through the old client's LinuxUpdater privilege path. No installed release, live feed, Apple
designated-requirement receipt or native migration was examined in this docs-only session. The issue's MSI/portable routing
statements remain required artifact checks rather than newly verified release facts.

All proposed API, schema, ownership and permission changes below are future contracts.
This documentation PR changes no runtime boundary and approves none of those changes.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Every recipe row identifies the exact installed legacy artifact/client version, updater config/manifest, target/architecture,
installation mode, expected old-client action, Keld artifact identity and current blocked/qualified verdict. Source-only rows
remain blocked for release.

Independent negative control: Label a row reachable using only a successful package build or config file; the missing installed-client/manifest receipt blocks
qualification.

### AC2

Windows x64 draw.io per-machine NSIS remains blocked until the exact KEL-270 T4d/MachineUacDirect lifecycle terminal artifact
and prerequisite approval exist. PerUserDirect tests, partial helper slices and local packaging cannot clear that gate.

Independent negative control: Mark Windows x64 reachable with a PerUserDirect receipt or one partial machine helper commit; the required-mode check fails.

### AC3

For the actual installed NSIS client, inspect app-update.yml publisherName and downloaded artifact signature behavior. Present
names require the exact accepted publisher result; missing names are recorded as a trust gap requiring an owner decision, not
treated as equivalent verification.

Independent negative control: Sign with a wrong publisher while the config lists a name, or delete publisherName and still assert the same enforced check; the
trust row fails.

### AC4

The bridge installer contract honors the old client's actual --updated, /S, --force-run and /D=<dir> invocation when applicable.
Mode, user-chosen root, relaunch identity and prompt behavior have native outcomes and KEL-254 binding evidence.

Independent negative control: Ignore /D, silently downgrade perMachine to perUser, or claim silent mode suppresses a required UAC consent prompt;
installation/root/authority checks fail.

### AC5

Published latest*.yml and the old installed config are checked together for artifact URL/hash/size, architecture/channel and
isAdminRightsRequired. The recipe does not infer elevate.exe usage solely from a perMachine source setting.

Independent negative control: Flip isAdminRightsRequired or substitute an arm64 artifact under x64 metadata; the recorded invocation/target oracle rejects it.

### AC6

MSI and portable rows have explicit ownership dispositions. If MSI-installed clients route to the NSIS bridge, choose approved
MSI detection/removal or filtering; portable has no assumed updater route and stays announcement/manual distribution only
without a new proven mechanism.

Independent negative control: Deliver an NSIS bridge to an MSI installation without the approved product-ownership treatment, or declare portable auto-update
from its artifact target name; the row stays blocked.

### AC7

windows-arm64 remains blocked until Pack/KEL-53 admit that exact target and the old latest-arm64 client/manifest path is
qualified. A Windows x64 or macOS arm64 result cannot be substituted.

Independent negative control: Reuse x64 updater receipts or omit latest-arm64 distinction; the target-identity check fails.

### AC8

The macOS row requires the exact installed Squirrel.Mac client, zip/latest-mac.yml path, signing/designated-requirement
acceptance and KEL-137/KEL-103 distribution artifacts. Same team/bundle ID is an inference until authoritative/native evidence
proves the actual check.

Independent negative control: Use local ad-hoc DMG success, matching bundle ID alone or unsigned ZIP download as proof of bridge acceptance; the
distribution/trust row remains blocked.

### AC9

Linux rows distinguish AppImage replacement from old-client Debian/RPM/Pacman package-manager installation. Serve bridge
packages only under an explicit owner decision with package/privilege/removal evidence; otherwise omit those rows. Snap/Flatpak
receive no invented direct updater route.

Independent negative control: Assign dpkg/rpm/pacman invocation to a Keld component or claim the AppImage result qualifies package-managed installs;
authority/target checks fail.

### AC10

A future release rehearsal proves download/verification, old-client action, interruption/failure recovery, correct retained user
data, installed Keld launch and the supported next-update policy on owned fixtures. Release and rollback plans have exact owner
approval before any live feed change.

Independent negative control: Treat old-client Promise success as installed Keld proof, hide a stale legacy installation or publish a redirect before
fixture/signature/mode gates pass; release qualification fails.

## 4. Design

### 4.1 Evidence row and release boundary

The bridge runs from the old installed Electron client. Its existing trust/configuration determines what it fetches and
executes. A Keld-built distribution installer may be the downloaded artifact, but no Keld application/Host updater component is
instructed to launch that installer or a privileged package manager. Once Keld runs, its own admitted update path is KEL-53 and
GH-581, not the legacy client's behavior.

Each row records: legacy app version and artifact digest; actual electron-updater artifact/version/digest; installed config
digest; platform/architecture/install mode; exact feed metadata digest and URL; intended artifact digest, signing/provenance and
native prerequisites; observed old-client process/flags; installed result; failure/rollback/user-data receipt; next-update
policy; verdict and owner. The recipe may name legacy feed URLs and installer paths as release evidence; it does not expose them
through the Keld runtime's path-free GH-581 interface.

Keep source receipt, artifact inspection and native execution separate. The five source configs show intended build settings,
while generated app-update.yml, latest*.yml and installed code are authoritative for the already-installed client. Pin and
inspect those exact bytes before qualifying any row.

### 4.2 Platform disposition matrix

| Installed client/target | Present verdict | Required owner/evidence |
| --- | --- | --- |
| Windows x64 per-machine NSIS | Blocked | T4d/MachineUacDirect terminal proof, publisher/flags/root receipts. |
| Windows MSI client | Blocked | Verify routing; select MSI removal or pre-feed filtering. |
| Windows arm64/latest-arm64 | Blocked | Exact Pack/KEL-53 target admission and legacy-client rehearsal. |
| Windows portable | Announcement/manual only | No updater-info path assumed; a new path needs separate approval. |
| macOS zip/Squirrel.Mac | Blocked | KEL-137/KEL-103, requirement/signature and native bridge receipts. |
| Linux AppImage | Blocked | Compatible artifact, replacement/failure and installed-Keld receipt. |
| Linux deb/rpm/pacman | Omitted pending decision | Bridge package ownership and old-client privilege proof. |
| Linux Snap/Flatpak | No direct path claimed | Distribution-owner alternative required. |

No row is qualified by this document. The issue records MSI carrying updater configuration but not being listed as the update
artifact, which could route an MSI install to NSIS; verify the actual installed/published bytes rather than assuming this
remains universal. Likewise, verify portable's absent update configuration before writing a public claim. Store/AppX behavior is
not inferred from the direct NSIS route.

### 4.3 Windows trust, flags and machine ownership

Natively inspect the old client's app-update.yml and signature-verification implementation. NsisUpdater returns no publisher
verification result when publisherName/config is absent; therefore 'updater verifies publisher' is conditional. The pinned
draw.io config lists two possible publishers; determine the actual generated list and accepted certificate identity rather than
copying only the config names.

The old client always adds --updated, adds /S for silent install, --force-run when requested, and /D only when installDirectory
is set. Web-installer --package-file is outside the proposed full-installer route and must be explicitly unsupported or
separately qualified. The Keld bridge artifact must preserve those invoked semantics without interpreting them as a right to
bypass protected root or consent. Inspect metadata isAdminRightsRequired; the source can select elevate.exe directly or fall
back on a launch error, so a static perMachine setting alone does not prove the actual process trace.

For draw.io perMachine, recommend keeping the row blocked until full MachineUacDirect is admitted. An owner may instead propose
a separate migration that intentionally leaves a stale machine install, but that requires an explicit new decision/cleanup plan
and does not clear this row's T4d criterion. The MSI packet must compare detection/removal versus filtering, identify
product/upgrade ownership, user-data retention and rollback, and pick one before release.

### 4.4 macOS and Linux boundaries

For macOS, obtain the actual installed client's Squirrel.Mac designated requirement and an authoritative explanation/native
verification receipt for the next artifact. Same developer team and bundle identifier are plausible ingredients, not a
sufficient verified acceptance claim in this record. PR1407's local ad-hoc macOS packaging has a distinct objective and
explicitly does not qualify Developer ID/notarized public distribution. Do not substitute a DMG proof for the old updater's ZIP
route or conflate its trust check with Keld's package verification.

AppImageUpdater source unlinks/replaces the old file, preserves or changes the filename according to its own rule and may
execute the replacement with environment flags. The rehearsal must include failure between those effects, permission/read-only
cases and user-selected filenames; a download-only receipt misses the destructive transition.

DebUpdater/RpmUpdater/PacmanUpdater run package commands through LinuxUpdater's root-or-privilege-wrapper path. This is behavior
of the legacy Electron client. A bridge decision must identify package naming/conflicts, trusted artifact, prompt/cancellation,
removal and rollback. Keld code receives no permission to spawn those tools; its ongoing update policy is separate. Recommend
omitting package-managed bridge rows until their owners choose and prove a supported route.

### 4.5 Rehearsal, failure and controlled release

The eventual rehearsal uses owned isolated installations of each exact legacy artifact and an owned non-production feed. Record
the complete old-client request, artifact verification, elevation/installer process family, target root, new installed Keld
identity and whether the old process exits/relaunches. Verify user documents/settings are retained using the approved
profile/identity migration decision; never infer that identical appId alone preserves profile ownership.

Interrupt or reject at download, signature, prompt, installer replacement and first launch. Record the actually remaining
executable/version and an operational rollback instruction for that row. A generic 'retry' is unsafe when the legacy client may
already have replaced files. Confirm the next Keld update policy: supported Host-owned updater, managed distribution or
explicitly unavailable. A one-time bridge does not imply automatic updates work in Keld.

Only the release owner can approve the exact artifact/metadata digests, audience/channel selection, rollback and live
publication. This task prepares reviewable documents and evidence requirements; it does not schedule or perform that
publication.

## 5. Boundaries

The future recipe/evidence ledger belongs to GH-583 and distribution owners; build translation GH-492, installed packaging
PR1407/KEL-137/KEL-103, KEL-254 and updater KEL-53 retain their owners. No runtime code, installer, updater feed, release asset
or signing configuration is changed here. Explicit owner packets are required for perMachine alternative, MSI routing and Linux
packages. Numeric rollout limits and retention/rollback procedures are release decisions after migration proof, not invented
defaults in this draft.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Collect pinned installed-client/config/feed artifacts for each row and reconcile them with the primary source paths. Keep all
rows blocked until artifacts exist; create the perMachine, MSI and Linux decision packets.

### T2

- [ ] Qualify distribution/trust prerequisites on owned fixtures: Windows T4d/root/publisher, macOS designated requirement/public
signing, target admission and package ownership. Do not clear one platform from another's proof.

### T3

- [ ] Rehearse each selected bridge route with actual flags, prompt/cancel, interruption and installed-Keld/user-data outcomes. Verify
the next-update policy and write target-specific rollback steps from observed states.

### T4

- [ ] Independently review source/artifact/native receipts and exact release metadata. Obtain the release owner's approval of any
future live publication; keep unsupported/omitted rows visible and do not publish from this docs batch.

## 7. Test plan

The independent oracle is the already-installed legacy client plus observed native installation state, not the new bridge
artifact's own success output. Verify actual client/package digests against the pinned source/package reference. Inspect
app-update.yml and latest*.yml separately; mutate publisher, hash, architecture, mode, /D, signature and admin-rights metadata
one at a time. Run prompt refusal, interrupted replacement and first-launch failure fixtures only within owned test
installations. Record parent/child process identity to prove whether the old client, installer or Keld component performed each
action. Validate installed Keld launch and user-data/profile behavior through their owning specs. No release/native test was run
during drafting.

Every acceptance criterion has an independently observable outcome and named negative control.
Record source commit, pinned reference, exact platform/runtime and raw receipt for every claimed result.
Documentation validation covers immutable source/readback, ten sections, controls and file limits.
Runtime, native and original-app tests are planned, not executed by this drafting session.
GitHub documentation CI cannot establish platform acceptance. Local `just ci`, hooks and machine
closeout were not run: the user restricted this work to GitHub. Official upstream GitHub sources
were read within that restriction; no Context7 query or external-machine evidence is claimed.

## 8. Review gates triggered

Release/distribution and security owners must approve each qualified row, trust/mode assumptions, user-data migration and
publication plan. KEL-270 T4d, KEL-137/KEL-103, KEL-254 and current KEL-53 admission gates are prerequisites where applicable. A
parked recipe is useful review material and does not unpark implementation, authorize legacy installation replacement or
redirect an update feed. Native evidence and exact-content approval remain mandatory.

Status remains draft. Implementation requires the section 10 decisions and approval of the exact contract.
Draft publication does not approve an upstream dependency or change a parked implementation schedule.

## 9. Perf impact

Documentation has no runtime effect. A future bridge normally transfers a full package and may temporarily retain legacy/new
artifacts; measure actual download/staging/install disk use and outage/relaunch time per target. No delta or memory savings are
claimed. Retention and cleanup must respect failed/rolled-back installation states and cannot be inferred from source file size.

## 10. Open questions (block implementation)

1. Release owner: choose which platform rows are worth qualifying after the migration proof; provide exact installed legacy
artifacts and generated updater metadata rather than source-config assumptions.

2. Machine-installation owner: supply full T4d/MachineUacDirect proof for the Windows x64 row, or propose a separately approved
alternative with explicit stale-install consequences. The existing row stays blocked.

3. Windows owner: resolve MSI detection/removal versus filtering and confirm actual publisherName/isAdminRightsRequired/flags
from the installed client and published metadata.

4. macOS distribution owner: provide Apple/Squirrel primary/native designated-requirement evidence and KEL-137/KEL-103
public-distribution artifacts; matching team/bundle ID remains an inference until then.

5. Linux owner: select bridge-only package artifacts or omit package-manager rows; specify package identity, privilege, data
retention and rollback. No Keld runtime package-manager path is authorized.

6. Profile/release owners: approve user-data continuity, rollback, next-update policy and exact production publication plan
after native qualification. No live feed action is included in this draft.

Independent review must refute or fix concrete findings on the published revision. Platform gaps
remain explicit; no unexecuted acceptance test is reported as passed.
