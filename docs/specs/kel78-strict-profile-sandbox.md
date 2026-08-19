# Spec: strict-profile OS sandbox and native-addon-worker proof

Status: draft
Linear: KEL-78 · Owner: GYLDLAB · Updated: 2026-08-19

Primary sources: [`kel78-primary-sources.md`](kel78-primary-sources.md)
Written against `origin/main` `67f39cdc898254f1e0c9cd50800f242ae7a4c493`

## 1. Goal & non-goals

Keld's strict profile claims **zero ambient OS authority** for every supervised
Bun role. A JavaScript permission shim cannot mediate native code already
running inside Bun: that code can open files and sockets, spawn, `dlopen`, and
use inherited handles. This specification turns that claim into a per-OS,
fail-closed, hostile-testable contract. A platform is **contained** only after
an archived proof names the shipped artifact, token/profile, remaining
handles/FDs, and hostile-probe output. Documentary availability of a primitive
is not containment.

Non-goals:

- No permissive profile whose purpose is to make Bun or a native addon start.
- No package-specific exception (including VS Code).
- No platform claim based on mocks, stubs, or prose.
- No claim that arbitrary unchanged addons work under denied authority.
- No syscall-interposition, container-runtime, or VM work without a proven
  corpus need.
- No rewrite of `docs/architecture/02-ipc.md` or `03-security.md` in this
  draft PR. Architecture 03 §4.2 remains the progressive sketch; this file is
  the implementation contract once approved (see §2).
- First implementation does not use opaque third-party addons.

## 2. Spec refs

- `docs/architecture/01-overview.md` §§1–2 (host owns OS resources; roles
  receive transport handles that cannot authorize another operation)
- `docs/architecture/03-security.md` §§1, 2, 4, 6 — **evidence only**. §4.2's
  progressive `sandbox_init` / restricted-token / landlock+seccomp list is
  weaker than this contract and names a deprecated macOS API (ledger M1).
  Updating §4.2 is a follow-up PR after this spec is approved, not this change.
- `docs/architecture/06-runtime-and-tooling.md` §1 (KEL-70 supervisor; KEL-75
  role spawn; KEL-78 owns real-OS sandbox admission)
- `docs/specs/kel75-principalized-bun-child-roles.md` AC6 and T6
- `crates/keld-runtime/src/lib.rs` (live supervisor; no sandbox)
- `crates/keld-guard/src/lib.rs` (`Principal` has no addon-worker variant)
- `crates/keld-guard/AGENTS.md` (default-deny; no engine special case)
- Ledger: [`kel78-primary-sources.md`](kel78-primary-sources.md)

This spec does not change kipc frame layout. It does not silently weaken
default-deny.

## 3. Acceptance criteria (binary, each becomes a test)

1. Given any supervised Bun role on an OS with no archived hostile proof, when
   a caller asks whether that OS is contained, then the only legal answers are
   `unverified` or `legacy` — never `strict` / "best effort secure".
2. Given a request to start a role in the `strict` state, when a required OS
   primitive is missing, the archived proof is missing, stale, or mismatched
   (artifact digest, token/profile, OS, archive id, or freshness), incomplete
   (missing §7 OS-containment rows), or layer-mismatched (a non-OS oracle
   recorded as OS containment), or admission otherwise fails, then the host
   does not start an unconfined child — including as a restart of a
   previously-`Strict` role — and MUST NOT return `Strict`. It returns a
   typed error that names the defect and the fix (install/enable the
   primitive, refresh or replace the archive, complete the OS-containment
   catalog against independent OS oracles, or declare `legacy` explicitly).
   Auto-downgrade to an unsandboxed `unverified` start is forbidden.
3. Given a role declared `legacy` by an explicit Keld profile key (not by
   Electron `sandbox`, `appSandbox: "off"`, package name, or architecture 03
   prose), when it starts, then it remains authenticated and
   `keld-guard`-checked, and every build/doctor/scoreboard/release metadata
   surface forfeits the zero-authority claim.
4. Given a successful `strict` spawn, when a process listing is taken, then the
   child has no raw host privilege handles: no inherited host filesystem,
   network, process-control, or update-staging handle. The only allowed extras
   are the authenticated app-link transport and supervisor log sinks. Those
   handles cannot authorize another operation.
5. Given the per-OS candidate in §4, when the named hostile probes in §7 run
   against the shipped artifact, then each probe records a **layer verdict**
   against that layer's independent oracle — not a JavaScript shim, and not
   another layer's success. Layers and oracles:

   | Layer | Independent oracle |
   |---|---|
   | OS containment | OS deny of the direct syscall/API (errno, NTSTATUS, sandbox violation). Job/PID kill is **not** this oracle — it is supervisor cleanup. |
   | Host protocol denial | typed `KELD-IPC-007` (or documented successor) from the host on the app-link |
   | Supervisor cleanup | descendants reaped, generation revoked, next legitimate spawn works |
   | Resource limits | worker hang/OOM hits the worker limits; host and webviews remain up |

   A JavaScript-shim deny is a failed OS-containment verdict for OS-layer
   probes. Host-protocol, supervisor-cleanup, and resource-limit passes do
   **not** prove OS containment and MUST NOT be the rows that admit `Strict`
   (see `admit` checks 8–9). Artifact digest, token/profile, remaining
   handles/FDs, and per-layer output are archived. A missing, incomplete, or
   layer-mismatched archive stays `unverified`.
6. Given a native addon, when the decision tree in §4 is applied, then the
   outcome is exactly one of: reuse in an addon-worker process, replace behind
   a guarded Rust host service, wait for an upstream Bun fix, or reject/exclude.
   There is no "run in-process under the app principal" option for untrusted
   native code.
7. Given an in-host Rust plugin, when it runs, then it is in the host TCB: the
   manifest constrains registered channels, not native syscalls. Untrusted
   native code requires a process boundary.
8. Given renderer sandbox status and Bun-role containment, when either is
   reported, then they are separate fields. A contained webview does not imply
   a contained Bun role.
9. Given host death, crash, abort, or hang of a strict child or addon worker
   on an OS whose §4 names a host-death reaper, when cleanup finishes, then
   descendants are reaped by that named mechanism, leftover grants/links are
   revoked, and the next legitimate spawn still works. Today §4 names Windows
   `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` (ledger W5) and Linux `CLONE_NEWPID`
   (ledger L1). macOS §4 names no reaper; this criterion does **not** apply
   to macOS until T2 records a primary-sourced mechanism. Process-group and
   `launchd` `SIGKILL` are not that mechanism (see §10 Q4).
10. Given the update staging directory, signing keys, and relaunch helper, when
    a strict child or addon worker tries to read or write them, then the OS
    denies the attempt. The updater stays a host-owned TCB path (KEL-53).

## 4. Design

### First-principles and reuse decision

- **Ownership:** `keld-host` remains the only general privileged process.
  `keld-runtime::Supervisor` already owns spawn, pipes, restart, and reap of
  one child (`crates/keld-runtime/src/lib.rs`) and does **not** apply an OS
  sandbox. This spec adds an admission gate *in front of* that supervisor. It
  does not replace KEL-70.
- **Trust:** A JS grant is not OS authority. `keld-guard` evaluates
  `(principal, channel, args)` for host-brokered calls. Native code inside Bun
  bypasses that path. Containment is therefore an OS profile on the child
  process, plus a separate process for untrusted native addons.
- **Lifecycle:** KEL-75 owns principal generation, link bind, and revocation.
  This spec owns whether a generation is allowed to start as `strict`,
  `legacy`, or stay `unverified`.
- **I/O:** The child receives `KELD_APP_LINK` and fixed-direction log sinks
  only. Live `spawn_piped` currently inherits everything else; that is a
  defect relative to this contract, not a granted exception.
- **Failure:** Missing primitives fail closed. There is no "best effort"
  sandbox. Leaving `strict` is an admission failure: no replacement child
  unless the app explicitly declares `legacy`.
- **Reuse:** Reuse KEL-70 `Supervisor` and KEL-75 role identity. Reuse
  `keld-guard` for brokered calls. Do not invent a fifth unique (the four
  uniques stay: prebuilt host, supervised Bun family, kipc, default-deny).
  Rejected alternatives: architecture 03 §4.2 `sandbox_init` (deprecated,
  ledger M1); ordinary AppContainer / Low IL / restricted token (ledger W2);
  seccomp or Landlock alone (ledger L5, L8); `CLONE_NEWNS` without an
  explicit host-path deny (ledger L9).
- **Compatibility fallback:** `legacy` is the only fallback. It is explicit
  and forfeits the claim.
- **Performance:** No speed claim. No rewrite justified by language choice.
- **Boundary change:** yes — permission model and later `unsafe` platform
  spawn. Public `Principal` gains an addon-worker variant in an
  implementation PR, not in this docs pass.

### Profile states

Three states. They are not a confidence slider.

| State | Who chooses it | Zero-authority claim | Child starts? |
|---|---|---|---|
| `unverified` | Default until a complete per-OS OS-containment archive exists | Forbidden | Only if the caller did not request `strict`. Reporting must say `unverified`. |
| `legacy` | Explicit Keld profile key | Forfeited, and the forfeit is printed | Yes: authenticated + guarded, unsandboxed |
| `strict` | Admission + complete OS-containment archive for that OS/artifact/profile | Allowed | Yes, only after admission |

```mermaid
stateDiagram-v2
    accTitle: Strict, legacy, and unverified profile states
    accDescr: Unverified is the default. Legacy requires an explicit Keld declaration and forfeits the zero-authority claim. Strict requires OS admission plus an archived complete OS-containment catalog with a matching token/profile digest. Leaving Strict is an admission failure that does not start an unsandboxed replacement. Legacy to Strict uses the same admit function as Unverified to Strict. Dropping the legacy declaration without a new complete archive returns Legacy to Unverified.

    [*] --> Unverified
    Unverified --> Legacy: explicit Keld legacy declaration
    Unverified --> Strict: admission plus complete OS-containment archive
    Strict --> Unverified: admission failure, no replacement child
    Legacy --> Unverified: declaration removed without a new proof
    Legacy --> Strict: declaration dropped plus same admit()
    note right of Strict
        Leaving Strict does not start
        an unsandboxed replacement
    end note
```

Rules:

- `unverified` is the live state of macOS, Windows, and Linux on this SHA.
  No hostile test has been run.
- `strict` cannot be reported from documentation.
- Renderer sandbox status is a separate field (AC8).
- Profile is never inferred from Electron facade options (`sandbox`,
  `appSandbox: "off"`), package name, or architecture 03's sandbox-off
  sketch. Electron-compat apps do not silently start `legacy`. If they
  need it, they declare the Keld profile key.
- Leaving `strict` (primitive missing, proof stale/incomplete/mismatched,
  or layer-mismatched) is an **admission failure**: no replacement child,
  typed error, `requested` stays `Strict` so a KEL-70 restart also fails
  closed. The only unsandboxed start is an explicit `legacy` declaration.
  Auto-downgrade into an unsandboxed `unverified` start is forbidden.
- `Legacy` → `Strict` uses the same `admit()` checks as
  `Unverified` → `Strict`. Dropping the declaration is not admission. A
  leftover Electron `sandbox` / `appSandbox: "off"` while `requested` is
  `Strict` fails closed (do not start).

### Authority surface the profile must cover

Every row is in scope for the threat model. "Denied" means the OS rejects the
direct attempt. The host broker may still perform the operation after
`keld-guard` allows it. A guard allow MUST NOT widen the child's OS profile
(no extra entitlement, capability SID, or seccomp allow as a consequence of
the grant).

| Resource | Strict child / addon worker |
|---|---|
| Files outside a reviewed role-private container | deny |
| Secrets / keychain / credential tokens | deny (cannot impersonate the user) |
| Network | deny (always: direct `connect`/`bind`/`socket`). Brokered net is host-side after `keld-guard`. Extra entitlements/SIDs are a permission-model gate, not child-ambient `connect()` |
| Devices (camera, mic, USB, GPS, …) | deny |
| Process / IPC control of host or siblings | deny |
| Inherited objects | none beyond app-link + log sinks |
| Code loading (`dlopen`, unsigned libraries, JIT) | deny except the recorded Bun JIT minimum on that OS |
| Brokers | only authenticated kipc to the host; no raw host object |
| Persistence / update staging / signing keys | deny |
| Descendants | same profile; no breakaway. Immediate kill is the supervisor-cleanup row, not an OS-containment pass |

### Addon-worker principal

Untrusted native code does not join `Principal::AppProcess` or
`Principal::Plugin`.

Destination shape (implementation PR, not this docs pass):

```text
Principal::AddonWorker { id: u32, generation: u32 }
```

Facts:

- Separate process, host-minted generation, private authenticated app-link.
- Bounded broker calls only; never a host privilege handle.
- Crash, OOM, abort, and hang have resource limits and do not take down the
  host or webviews.
- In-host Rust plugins stay `Principal::Plugin` and join the host TCB
  (architecture 03 §1). Their manifest constrains channels, not syscalls.

Decision tree for a native addon (exactly one outcome):

1. **Reuse in worker** — the addon is loaded only inside the sandboxed
   addon-worker process; JS talks to it through host-mediated calls.
2. **Replace behind a guarded Rust service** — the capability moves into
   `keld-native` / a reviewed plugin and is guard-checked.
3. **Upstream Bun fix** — parked; the addon is not shipped as `strict`.
4. **Reject / exclude** — default when 1–3 do not apply.

There is no fifth outcome that loads the addon into the primary Bun role
under `strict`.

### macOS candidate

Required:

- The launched Bun/helper is a **separately signed** binary with its own
  App Sandbox entitlements (`com.apple.security.app-sandbox`, ledger M2–M3).
  It MUST NOT use `com.apple.security.inherit` from `keld-host`. The host is
  the TCB; inherit copies the parent's sandbox (ledger M5) and
  `posix_spawn`/`NSTask` cannot put a helper in its own sandbox (ledger M6).
- Hardened Runtime may be required for notarization; it is **not** sufficient
  for `strict` (ledger M8).
- Required helper keys: `com.apple.security.app-sandbox` plus only the
  recorded JIT minimum if a Bun-start failure forces it. Any other App
  Sandbox entitlement on that helper is an unexpected grant.
- Privilege-separated helpers, if any, are XPC services with their **own**
  sandbox, private to the host bundle, not root (ledger M6). The host
  enumerates every XPC service, Powerbox path, and security-scoped bookmark
  it uses. Default enumeration for a Bun role is empty.
- Powerbox / user-selected files / security-scoped bookmarks stay on the host
  (ledger M4, M7). A Bun role does not get
  `com.apple.security.files.user-selected.*` or bookmark entitlements.
- `com.apple.security.temporary-exception.*` is an unexpected grant.
- Device and personal-information entitlements are absent unless a recorded
  Bun-start failure names them. Each such entitlement is a permission-model
  review gate.
- Network entitlements (`com.apple.security.network.client` /
  `network.server`) are always absent. A recorded Bun-start failure MUST NOT
  add them. A `keld-guard` network grant MUST NOT add them either — that
  would be ambient `connect()` for native code. Direct net stays OS-denied.
- JIT: `com.apple.security.cs.allow-jit` is allowed only if a recorded
  Bun-start failure proves it is required. `allow-unsigned-executable-memory`
  and `disable-library-validation` stay denied until the same bar is met.

Fail closed when: the binary is not App Sandboxed; the helper inherits from
the host; `temporary-exception.*` or another unexpected entitlement is
present; or XPC/Powerbox grants cannot be enumerated.

Packaging: signed `.app` with the reviewed entitlements embedded in the
signature. Unsigned or ad-hoc signed children are `unverified`, never `strict`.

Host-death descendant reap is **not** specified here. T2 MUST name the
mechanism from a primary source before AC9 applies on macOS. Process-group
and `launchd` `SIGKILL` are not claimed (see §10 Q4).

`sandbox_init(3)` is deprecated (ledger M1) and is not the candidate.

### Windows candidate

Required:

- Explicitly constructed **zero-capability LPAC**: package SID +
  `SECURITY_CAPABILITIES` with `CapabilityCount = 0` +
  `PROCESS_CREATION_ALL_APPLICATION_PACKAGES_OPT_OUT`
  (`PROC_THREAD_ATTRIBUTE_ALL_APPLICATION_PACKAGES_POLICY`). Ledger W2–W3.
- Reviewed ACLs on runtime, profile, and data objects: access is the
  intersection of user SID and package SID (ledger W4). Capability SIDs
  (`registryRead`, `lpacCom`, network, …) are absent unless a recorded
  experiment proves them required. Each such SID is a **permission-model
  review gate** (same class as macOS extra entitlements). A network SID
  MUST NOT be added because of a `keld-guard` net grant.
- Handle allowlist via `PROC_THREAD_ATTRIBUTE_HANDLE_LIST`. Default
  `bInheritHandles = FALSE` (ledger W6).
- Job object for descendants: no `BREAKAWAY_OK` / `SILENT_BREAKAWAY_OK`;
  `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` for host-death cleanup (ledger W5).
  The job is **not** the authority sandbox.

Insufficient alone (do not admit `strict`): ordinary AppContainer, MSIX
packaging, Low integrity level, `CreateRestrictedToken` without LPAC.

Fail closed when: LPAC opt-out is missing; any capability SID is present
without a recorded experiment; a host handle leaks; a child breaks away from
the job.

### Linux candidate

Required, together:

1. User, mount, PID, and network namespaces (`CLONE_NEWUSER`, `CLONE_NEWNS`,
   `CLONE_NEWPID`, `CLONE_NEWNET`). Ledger L1–L4.
2. After `CLONE_NEWNS`, an **explicit host-path deny** policy. Creating a
   mount namespace copies the parent's mount list (ledger L9); `CLONE_NEWNS`
   is not a filesystem deny. The policy MUST:
   - deny create/read/write on host paths outside the reviewed role-private
     container (home, `/etc`, update staging, sibling-role paths, host TCB
     paths);
   - still allow the role-private paths that role is supposed to use;
   - be proven by tests: host-path open/create fails with an OS error;
     role-private open/create succeeds;
   - fail closed: if the deny cannot be applied, do not admit `strict`.
   The archive names the mechanism actually used. Item 2 is a
   **mount-table** change only: bind-mount allowlist of the role container,
   covering or unmount of host trees, or `pivot_root` (or an equivalent
   mount operation that removes host paths from the child's mount list).
   Landlock MAY only **stack** on top (item 6, ledger L8). It MUST NOT
   substitute for item 2.
3. `prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0)` before seccomp. Ledger L5.
4. Empty capability sets and dropped bounding set (`PR_CAPBSET_DROP`).
   Ledger L6.
5. Seccomp-BPF that denies direct fs/net/spawn/ptrace/mount not needed for
   the authenticated link, inherited by descendants (ledger L7). The filter
   MUST deny descendant `clone`/`unshare` with `CLONE_NEWUSER`, `setns` into
   a user namespace, and `clone3` (so a grandchild cannot create a new user
   namespace). Allowing those calls while claiming "no new user namespace"
   is a failed descendant proof.
6. Landlock filesystem/network rules when the kernel supports them (ledger L8).
   Additional layer **on top of** items 1–5, including on top of item 2's
   mount-table deny. Not a substitute for 1–5 and not an implementation of
   item 2.

Unavailable user namespace: `clone`/`unshare` with `CLONE_NEWUSER` fails
(`EPERM`, `EACCES`, `EUSERS`, `ENOSPC`, or LSM deny). Admission then
**refuses** a `strict` start. A privileged launcher that creates namespaces
on the child's behalf is a TCB addition and needs its own human review; it
is not implied by this spec (see §10).

Seccomp alone, Landlock alone, or `CLONE_NEWNS` without the host-path deny
cannot admit `strict`.

Fail closed when: any of 1–5 is missing; the host-path deny cannot be
applied; Landlock is offered as the item-2 deny (stack-only, never a
substitute); a capability remains; a descendant escapes the PID namespace
or the seccomp filter; a descendant creates a user namespace via
`clone`/`clone3`/`unshare`/`setns`. `SCM_RIGHTS` is a runtime hostile
probe (ledger L5), not an `admit()` primitive.

### Types and channels (sketch; not implemented on this SHA)

```text
enum ProfileState { Unverified, Legacy, Strict }

/// SHA-256 of the launched binary. Compared to the archived proof.
struct ArtifactDigest([u8; 32]);

/// SHA-256 of the token/profile this spawn will apply.
/// macOS: App Sandbox entitlement set on the separately signed helper
///   (no inherit-from-host; JIT; enumerated XPC/Powerbox/bookmarks).
/// Windows: LPAC SECURITY_CAPABILITIES (count + SIDs) + All-Application-
///   Packages opt-out + reviewed ACL generation.
/// Linux: namespace set + host-path deny mechanism + seccomp filter
///   generation + capability set + Landlock ABI if present.
/// A digest taken under a different entitlement / LPAC / capability set
/// MUST NOT admit Strict for this spawn. Not interchangeable with
/// ArtifactDigest: same binary, different profile → different value.
struct ProfileDigest([u8; 32]);

struct ProofIdentity {
    os: Os,
    artifact_digest: ArtifactDigest,
    profile_digest: ProfileDigest,
    archive_id: ArchiveId,
}

struct AdmissionRequest {
    role: RoleInstance,          // KEL-75 generation
    requested: ProfileState,     // never inferred from package name or Electron sandbox/appSandbox
    artifact_digest: ArtifactDigest,
    profile_digest: ProfileDigest, // candidate this spawn will apply
    proof: Option<ProofIdentity>,
}

enum AdmissionError {
    // KELD-RUNTIME-0xx in the implementation PR; Display names the fix
    PrimitiveUnavailable { os, primitive, fix },
    UnexpectedGrant { os, grant, fix },
    HandleLeak { description, fix },
    ProofMissing { os, fix },    // requested Strict; proof absent / archive empty
    ProofStale { os, archive_id, reason, fix },
    ProofMismatch { os, field, expected, found, fix }, // artifact, profile, OS, archive_id
    ProofIncomplete { os, missing, fix }, // §7 OS-containment rows absent
    ProofLayerMismatch { os, probe, expected_layer, recorded_layer, fix },
}

fn admit(req: AdmissionRequest) -> Result<ProfileState, AdmissionError>
```

`admit` MUST NOT return `Strict` unless every check below passes:

1. `requested` is `Strict`.
2. The §4 primitives for this OS are present (including Linux host-path deny).
3. `proof` is `Some`.
4. `proof.artifact_digest` equals `req.artifact_digest`.
5. `proof.profile_digest` equals `req.profile_digest`.
6. `proof.os` is this host.
7. The archive identified by `proof.archive_id` is **fresh** for this artifact
   digest and profile digest: it covers this exact artifact and token/profile,
   and the §4 policy admission will apply. A newer binary, a different
   profile digest, or an archive that predates a required primitive/policy
   change is stale.
8. The archive contains a **complete §7 OS-containment catalog**: every
   hostile-catalog row whose Layer is `OS containment` has a recorded pass.
   Host-protocol, supervisor-cleanup, and resource-limit rows MAY be present;
   they do **not** satisfy this check and do **not** prove OS containment.
9. Each recorded OS-containment pass used that row's independent OS oracle
   (deny of the direct syscall/API). A JavaScript-shim deny, a
   `KELD-IPC-007` / host-protocol pass, a supervisor reap, or a resource-limit
   hit recorded in an OS-containment slot is `ProofLayerMismatch`.

Otherwise the result is not `Strict`:

| Defect | Error |
|---|---|
| `proof` is `None` or the archive is empty | `ProofMissing` |
| artifact digest, profile digest, OS, or `archive_id` does not match | `ProofMismatch` |
| archive is not fresh for this digest/profile | `ProofStale` |
| one or more §7 OS-containment rows missing | `ProofIncomplete` |
| an OS-containment slot used another layer's oracle | `ProofLayerMismatch` |
| required primitive or host-path deny missing | `PrimitiveUnavailable` |
| leftover Electron `sandbox` / `appSandbox: "off"` while `requested` is `Strict`; inherit-from-host; unexpected entitlement/SID | `UnexpectedGrant` |

An archive that contains only protocol, cleanup, or limit rows is
`ProofIncomplete` (OS-containment rows absent). If those other-layer
results were written into OS-containment slots, it is
`ProofLayerMismatch`. Neither admits `Strict`.

`Legacy` does not require a strict archive. `Legacy` → `Strict` uses this
same `admit()` — dropping the declaration is not admission. Default /
`Unverified` never becomes `Strict` through this function. A leftover
Electron `sandbox` / `appSandbox: "off"` while `requested` is `Strict` is
`UnexpectedGrant` (do not start). Leaving `Strict` is this function
returning an error: no replacement child. T1 implements this state machine
without claiming OS containment.

Capabilities / manifest: a future `keld.permissions.jsonc` / `keld.config.ts`
key may declare `profile: "strict" | "legacy"`. The key does not exist today.
Adding it is a permission-model + public-API review gate. Electron
`sandbox` / `appSandbox` is not this key and MUST NOT be read as `legacy`
or `strict`. No VS Code or package-name branch.

Wire/protocol: none. `KELD_APP_LINK` stays `<endpoint>#<64 hex chars>`.

### Packaging and reporting

| Surface | `strict` | `legacy` | `unverified` |
|---|---|---|---|
| `keld build` output | printed only after a complete OS-containment archive exists | printed "zero-authority claim forfeited" | printed "unverified — no OS proof" |
| `keld doctor` | same | same | same |
| Scoreboard / release metadata | same | same | same |

Renderer sandbox is a different column.

## 5. Boundaries

- Implement in later slices: `keld-runtime` (admission + handle stripping),
  `keld-guard` (addon-worker principal), `keld-pack` (entitlements / LPAC
  profile), `keld-cli` (doctor/build text), optional `keld-native` (synthetic
  addon fixture).
- This PR must not touch: `docs/architecture/02-ipc.md`,
  `docs/architecture/03-security.md`, `docs/research/`, KEL-74 branches,
  PR #21 scoreboard/llms files, PR #30 updater architecture files,
  workspace `Cargo.toml`, kipc frame layout.
- Must not add a permissive default to make Bun start.

## 6. Tasks (each ≈ one PR; ordered; no placeholders — vertical slices only)

- [x] T0: Draft this spec + primary-source ledger. All OS proofs unverified.
- [x] T1: Admission API + profile-state reporting. Synthetic in-process
      *probe binary* (not a third-party addon). Default state `unverified`.
      Fail closed when `strict` is requested (`ProofMissing` /
      `ProofIncomplete` / `ProofLayerMismatch` / `ProofMismatch` including
      profile digest). No OS containment claim.
- [ ] T2: macOS App Sandbox admission + hostile archive for the synthetic
      fixture on a **separately signed** helper (no `inherit` from the
      host). JIT entitlements only if a recorded Bun-start failure
      requires them. Enumerate `temporary-exception.*` as unexpected.
      Name the host-death reaper from a primary source before AC9 applies
      on macOS; do not invent process-group / `launchd`.
- [ ] T3: Windows zero-capability LPAC + ACL + handle allowlist + job
      descendant proof.
- [ ] T4: Linux namespace + explicit host-path deny (role-private paths still
      work; mount-table change or `pivot_root`, not Landlock) +
      `no_new_privs` + cap drop + seccomp that denies `clone3` / `setns` /
      `unshare`+`CLONE_NEWUSER` (+ Landlock **stack only** when present)
      and unavailable-userns fail-closed proof. `SCM_RIGHTS` is a T4
      runtime probe, not an admit primitive.
- [ ] T5: Crash/hang/OOM cleanup and updater-boundary probes on each OS
      that passed T2–T4.
- [ ] T6: Doctor / build / release metadata surfaces. Then KEL-75 T6 may
      permit a `strict` release claim for those OSes only.
- [ ] T7: Synthetic native-addon worker process (still not opaque
      third-party code) exercising the decision tree.

## 7. Test plan

No test in this docs pass is an OS proof. Future fixtures must hit the real
OS. Mocks may test the admission state machine only.

| AC | Future fixture | Independent oracle |
|---|---|---|
| 1 | doctor/build metadata unit | printed state string is `unverified`/`legacy`/`strict` |
| 2 | admission with primitive removed; `Strict` with missing, stale, or mismatched proof (wrong artifact digest / profile digest / OS / archive id); incomplete OS-containment catalog; layer-mismatched archive (protocol/cleanup/limit rows only, or those oracles in OS slots); Strict restart after stale/missing proof; leftover Electron `appSandbox: "off"` while requested Strict; Legacy→Strict without `admit()` | typed `PrimitiveUnavailable` / `ProofMissing` / `ProofStale` / `ProofMismatch` / `ProofIncomplete` / `ProofLayerMismatch` / `UnexpectedGrant`; no child PID (including no unsandboxed replacement); result is not `Strict` |
| 3 | explicit Keld `legacy` spawn (Electron `sandbox` / `appSandbox` alone is not this declaration) | child runs; metadata forfeits the claim |
| 4 | `/proc/pid/fd`, `lsof`, or `GetProcessHandleCount` + handle dump | only app-link + log sinks |
| 5 | hostile catalog below | **per-layer** oracle in that row; JS shim deny fails OS-containment probes; host-protocol / cleanup / limit passes do not mark the OS contained and do not admit `Strict`; missing/incomplete/layer-mismatched archive stays `unverified` (`ProofIncomplete` / `ProofLayerMismatch`) |
| 6 | synthetic addon matrix | one of the four outcomes; in-process load fails closed under `strict` |
| 7 | plugin vs worker | plugin has no OS sandbox; worker does (once T7 exists) |
| 8 | doctor JSON | two distinct fields |
| 9 | kill host / abort child / hang on an OS whose §4 names a reaper (Windows job; Linux PID NS). macOS: not claimed until T2 | no leftover descendants; next spawn works. macOS stays unverified for this AC |
| 10 | open update staging / key path from child | OS deny |

Hostile catalog (each is a real syscall/API from the child or addon worker).
Each row is one layer. Do not attribute every outcome to the OS.

| Probe | Layer | Must observe |
|---|---|---|
| Direct filesystem | OS containment | create/read of a **host** path outside the role container fails with an OS error; create/read of a **role-private** path succeeds |
| Direct network | OS containment | `connect`/`bind`/`socket` fails with an OS error even when a `keld-guard` net grant exists. Brokered net is host-side; a reviewed grant MUST NOT change this observation |
| Direct shell / spawn | OS containment | `exec`/`CreateProcess`/`posix_spawn` of a helper fails with an OS deny, **or** the descendant is equally sandboxed (same App Sandbox / LPAC / namespace+seccomp+host-path profile). Job/PID kill of an unsandboxed helper is supervisor cleanup, not this pass |
| Inherited handles | OS containment | leftover host FD/HANDLE cannot open host files, the update dir, or sibling links |
| SCM_RIGHTS | OS containment | `recvmsg` of a usable host FD, or use of that FD on a host path, fails the archive. The host MUST NOT send ancillary FDs; the child MUST reject/close them. Runtime probe, not an `admit()` primitive (ledger L5) |
| Descendants | OS containment | grandchild has the same profile; no job breakaway; no new user namespace (`clone`/`clone3`/`unshare`/`setns` with `CLONE_NEWUSER`). Job/PID reap is the supervisor-cleanup row, not this pass |
| Broker bypass | OS containment | raw use of a host object the broker would have checked is denied by the OS |
| Token theft | OS containment | cannot impersonate the interactive user (macOS keychain / Windows user token / Linux host UID 0) |
| Addon escape | OS containment | synthetic native code cannot `dlopen` unsigned host code or join the host address space |
| Crash cleanup | Supervisor cleanup | abort/OOM/hang leaves no descendant and revokes the generation (supervisor/job/PID-namespace reap — not an OS-containment pass by itself) |
| Updater boundary | OS containment | cannot read/write staging, signatures, or the relaunch helper |
| Protocol confusion | Host protocol denial | foreign `HELLO`, stale token, or confused deputy on the app-link is `KELD-IPC-007` / typed host deny — not an OS-containment pass |
| Resource | Resource limits | intentional hang and allocator blow-up hit the worker limits, not the host — not an OS-containment pass |

Anti-flake: bind port 0; use temp dirs; await supervisor events (no sleep-sync);
run crash cases out of process. Platform-only tests report other OSes as
`unverified` rather than skip-and-claim.

Negative control: temporarily remove the LPAC opt-out, the App Sandbox
entitlement, `CLONE_NEWUSER`, or the host-path deny (leaving `CLONE_NEWNS`
as a copied host mount table) and confirm the `strict` tests fail.
Also remove descendant-namespace denies (`clone3` / `setns` / descendant
`CLONE_NEWUSER`) and confirm the descendant OS-containment probe fails.
Role-private path success is a required positive control, not a substitute
for the host-path deny.

## 8. Review gates triggered

- unsafe: **yes, in later implementation PRs** (platform spawn / namespace /
  token construction). None in this docs PR.
- public API: **yes, later** (`Principal::AddonWorker`, profile declaration).
  None in this docs PR.
- permission model: **yes** — this spec is the strict-profile permission
  contract. Human sign-off required before implementation.
- dependency: none in this spec.
- wire protocol: none in this spec.

Removal / rollback: delete the admission gate and keep KEL-70 unsandboxed
supervision; every surface returns to `unverified`. Do not keep a half-applied
profile.

## 9. Perf impact

none for this documentation pass. Later sandbox spawn is a cold path.
Architecture 01 §5 budgets are not claimed to move until an attributed
end-to-end measurement exists.

## 10. Open questions

1. If unprivileged user namespaces are unavailable, may a reviewed privileged
   launcher join the TCB, or is `legacy` / refuse the only option?
2. What is the minimum experimentally required Bun JIT entitlement set on
   each OS? Recorded Bun-start failures decide; do not pre-grant.
3. After approval, who updates architecture 03 §4.2 to replace the
   `sandbox_init` / restricted-token / landlock+seccomp sketch with this
   contract? (This PR does not.)
4. KEL-75 still asks this spec to name the per-OS host-death reaping
   mechanism. §4 already names Windows `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`
   (W5) and Linux `CLONE_NEWPID` (L1); AC9 applies there. macOS §4 names
   none. Tentative macOS leads (process group / XPC `SIGKILL` by `launchd`)
   and Linux `PR_SET_PDEATHSIG` are **not** claimed. T2 MUST pick the macOS
   reaper from a primary source before AC9 applies on macOS. Do not invent
   one here.
