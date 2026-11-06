# Spec: Persistent Strict role-private user data and logs
Status: draft
Linear: GH-525 (#517) · Owner: @0monish (exact contract approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; documentation only, do not merge.

## 1. Goal & non-goals

Define a persistent per-user data/log root that the approved Strict role can access with raw node:fs, while the operating
system denies access outside that role's reviewed authority. Keep stable application data distinct from launch stages and
webview profile storage.

No import of existing Electron data, automatic migration/deletion, node:fs proxy, new app.fs.* grant, browser-profile
relocation or new claim of Strict qualification. The issue remains parked for implementation; the user has authorized this
future specification only.

## 2. Spec refs

- [Owning issue GH-525](https://github.com/gyldlab/keld/issues/525); current public owner decisions remain authoritative.
- [KEL-78 role containment owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/kel78-strict-profile-sandbox.md).
- [Current Linux role-root validation and mount](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-runtime/src/linux_strict.rs).
- [Runtime boundary rules](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-runtime/AGENTS.md).
- [Validated identity and browser-store owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/kel135-persistent-profile-identity.md).
- [Host app-facts owner](https://github.com/gyldlab/keld/blob/c2dd2ff53767adf20dc0e91d276310095fe31acf/docs/specs/gh452-app-session-facts.md).
- [App path overrides](https://github.com/gyldlab/keld/blob/2b5a4f978d84ebd7ddcbda3fb1c69c8a602bb731/docs/specs/gh522-app-path-overrides.md).
- [Explicit Linux profile qualification](https://github.com/gyldlab/keld/blob/62653e00c951570565728db1d8527f93e607c935/docs/specs/gh1128-linux-profile-qualification.md).
- [Explicit Windows profile qualification](https://github.com/gyldlab/keld/blob/62653e00c951570565728db1d8527f93e607c935/docs/specs/gh1187-windows-profile-qualification.md).

Verified source baseline: KEL-78 owns reviewed role-private storage and forbids using a broker grant as proof of raw syscall
containment. Current LinuxStrictProfile validates a role_root and bind-mounts it at /app; that existing mount is not evidence
of persistence across stage cleanup. KEL-135 separately owns Host-selected webview stores and validated app identity. GH522
path overrides do not move those stores. No real persistent-role-root probe ran in this session.

The proposed contract below is future work. This publication changes no runtime boundary and grants no implementation or
predecessor approval.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given one approved Strict launch, when getPath('userData') and getPath('logs') are read, then both map to existing
directories within that role's admitted private storage and a raw node:fs write/read succeeds there.

Independent negative control: Return a pleasant path string or only add an app.fs.* grant without provisioning OS-visible
access; the real raw positive control must fail.

### AC2

Given an existing readable/writable Host canary outside the role root and a different application's private root, when the
Strict child attempts raw access, then the OS denies both and the canaries remain unchanged.

Independent negative control: Run the same child without the intended containment/ACL/mount rule; the independent negative
fixture must expose outside access.

### AC3

Given a file written under a validated stable application/role identity, when the session exits, its stage is cleaned and the
same app is relaunched, then the marker persists under the same admitted data identity.

Independent negative control: Place data under the per-launch stage or derive the namespace from PID/generation/stage nonce;
the relaunch control loses or relocates it.

### AC4

Given two apps with the same display name and two roles of one app, when storage is selected, then their private namespaces
are distinct unless an explicit shared-storage contract exists.

Independent negative control: Key the root only by app display name or copy the primary's root into every named role.

### AC5

Given a symlink/reparse point, wrong owner, unsafe permissions or replaced ancestor in a candidate root, when admission runs,
then it fails before granting child access or mutating someone else's data.

Independent negative control: Validate only a path string, follow a changed ancestor or accept a marker from another
identity.

### AC6

Given getPath/setPath/setAppLogsPath operations, when a facade value changes, then it cannot widen the frozen OS profile or
relocate the engine store; unauthorized raw access remains denied.

Independent negative control: Treat a new facade path or sessionData override as a new Host storage grant.

### AC7

Given an approved data-root revision and an app upgrade, when executable bytes change but validated app identity remains
valid, then ordinary persistent data is preserved without treating it as executable staging or profile evidence.

Independent negative control: Delete persistent data with the old stage or infer new Strict qualification solely from data
continuity.

### AC8

Given a process still retaining an open data handle during shutdown, when cleanup is attempted, then the existing supervisor
owns retirement; data is not recursively removed or reallocated on a mere caller timeout.

Independent negative control: Mark the root reusable/deletable from a rejected Promise before the owned process is retired.

### AC9

Given the chosen profile, when migration/reporting describes userData, then Strict explicitly describes role-private raw
access and outside denial, while Legacy describes ordinary paths and forfeited containment.

Independent negative control: Label Legacy raw access as sandboxed or advertise a guarded fs root as equivalent to
OS-contained raw access.

### AC10

Given each proposed OS lane, when Strict storage support is published, then a real same-identity relaunch, outside-denial and
cross-app control is attached to the exact package/runtime/profile tuple.

Independent negative control: Promote a Linux mount model or a Windows Job cleanup pass to a macOS/Windows
filesystem-containment result.

## 4. Design

### 4.1 Three different identities

Reuse the package-verifier's validated app identity and the existing Host role declaration. The persistent data key must be
stable across process generations and ordinary upgrades, distinct across apps/roles, and scoped to the current OS user. A
role root is not a webview ProfileIdentity or Apple store UUID. The KEL-78 owner defines its one namespace encoding; this
document must not introduce an unrelated hash/helper or accept names from renderer/Bun input.

### 4.2 Per-OS location proposal, pending KEL-78 amendment

macOS candidate: the actually admitted signed role's OS-provided private container, with a stable data leaf under its
application-support area and logs inside that same private authority. Windows candidate: a Host-validated per-user
LocalAppData Keld role-data namespace, with a dedicated app/role leaf whose ACL admits only the approved Host and role
identities. Linux candidate: a Host-validated absolute per-user data root with the same stable namespace, mounted at an
approved private child path. These are candidate selectors, not verified APIs or current qualified locations; T1 must name
exact native selectors, leaf encoding and child-visible paths in the KEL-78 amendment.

### 4.3 Persist data independently of /app staging

Current Linux /app is the role-root mount. Do not replace executable/application staging with mutable persistent data or
place a Host symlink inside the stage as a shortcut. The owner must choose a reviewed persistent submount or an equivalent
private-root layout that keeps executable/source identity and data lifetime separate. Native admission must prove that stage
cleanup cannot traverse into the persistent source. macOS signing/container and Windows LPAC ACL qualification remain their
existing owners' gates.

### 4.4 Provision, validate, then publish facts

Before spawning the role, the Host creates or opens only the approved identity-bound leaf through the existing validated
filesystem owner. Validate ancestors, actual owner/ACL/mode and an identity/version marker where the platform contract uses
one. Refuse collisions and hostile replacement before exposing the root. Pass the child-visible path through GH452/GH570 fact
ownership only after profile admission. If preparation fails, no partial path fact claims usable storage and no broad
fallback is selected.

### 4.5 Raw access is the positive control

The caller uses ordinary node:fs in its private container; no broker call is involved. This is why an app.fs.* grant is
neither necessary evidence nor a replacement containment mechanism. Strict outside access remains denied even if a brokered
read would be permitted. Shared app/role data, secrets and imported Electron profiles require separate owner decisions. A
returned path never grants authority to another role.

### 4.6 Lifetime and destructive operations

Persistent user data survives ordinary stage cleanup, app restart and update. Only an explicit later data-removal/reset
contract can delete it; this issue adds no such command. Keep existing process lifetime/custody and concurrency leases where
they apply. Ownership markers are not proof that no process retains a handle. Crash/recovery does not reset stable data
identity, and a failed profile proof must not silently fall back to Legacy.

### 4.7 Compatibility and reporting

GH522 retains override behavior and error shapes, but an override cannot change the captured OS profile. GH452 reports the
approved actual paths. GH590/GH591 own migration analysis/report/write behavior and must use one profile-aware explanation:
raw role-private access under Strict; outside calls need the existing guarded facade or dialog grant; explicit Legacy
forfeits containment. No user's existing data is moved, imported or erased by generating that report.

## 5. Boundaries

Future implementation is a coordinated KEL-78 storage/profile amendment with the existing identity/filesystem/facts and
migration owners. This document owns requirements and handoff, not a second sandbox implementation. No change to KEL135
stores, permission schema, OS default roots or another role's custody.

This publication changes only `docs/specs/gh525-strict-persistent-role-storage.md` and related grouped-PR/issue handoffs.
Shared architecture, policy, generated contracts, implementations and other owners' documents are not edited.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Freeze exact per-OS persistent selectors, namespace, child-visible paths, native validation and profile digest changes in the KEL-78 amendment; keep parked scheduling and native approval explicit.

### T2

- [ ] Provision one platform's identity-bound persistent root and deliver its actual facts with raw inside/outside and collision/replacement controls.

### T3

- [ ] Qualify same-app relaunch, different-app/role isolation, upgrade and stage-cleanup behavior on each intended OS without reusing one platform's receipt for another.

### T4

- [ ] Integrate the existing migration/report and path-facade consumers, preserving browser-store identity and explicit Legacy wording; publish exact supported lanes only.

## 7. Test plan

AC1/2 use real child node:fs calls plus preverified Host and different-app canaries; a missing canary is not an OS denial.
AC3/7 record identity and content across stage deletion and clean relaunch. AC4 uses same-name distinct signed identities and
separate role declarations. AC5 forces path replacement before native admission. AC6 independently inspects the frozen
profile and store identity. AC8 observes owned process/handle retirement without deleting persistent data. AC9 compares exact
profile-specific report output. All native runs remain unperformed.

Each acceptance criterion has its own observable and negative control above. Documentation verification covers all ten
sections, source-versus-proposal wording, file limits and exact GitHub readback. GitHub CI belongs to the exact published
head. Native/runtime/conformance/OS outcomes remain unrun until the implementing owner executes them; green documentation
checks do not prove those outcomes.

The user restricted this session to GitHub. Local `just ci`, hooks and machine session-closeout were not run. Official
immutable upstream GitHub sources are used where relevant; Context7 was not used under that source restriction. No
external-machine or native receipt is fabricated.

## 8. Review gates triggered

Permission-model, package/role identity, native profile and public path-fact review. Wire/unsafe/dependency gates apply only
to explicitly approved implementation changes; none is silently granted here.

Draft publication is not exact-content approval. Pending predecessor approvals and native qualification remain separate
gates.

## 9. Perf impact

Cold root admission and existing persistent I/O only. No new timer or directory scan. Bound marker/schema parsing through the
existing filesystem owner; do not claim performance or OS containment from this document.

## 10. Open questions (block implementation)

1. Approve a KEL-78 amendment naming exact persistent per-user selectors, identity encoding and child-visible layout for
every supported OS.

2. Resolve current Linux /app staging versus persistent submount ownership, including stage-cleanup safety; qualify macOS
container and Windows LPAC ACL behavior separately.

3. Define concurrency/lease behavior for simultaneous sessions of the same validated app/role identity. Distinct app/role
identities retain distinct private roots. Until same-identity concurrency is approved, refuse the conflicting storage
admission; never salt the persistent namespace with a session or generation identifier or allocate a replacement root.
Confirm that a later admitted relaunch reads the marker written before the conflict.

4. Keep implementation parked until scheduled and attach real raw-access/relaunch/isolation evidence before any Strict
support claim.

Independent review must cover the final published revision. Every remaining owner/native gate stays explicit; this document
is not a completed feature.

