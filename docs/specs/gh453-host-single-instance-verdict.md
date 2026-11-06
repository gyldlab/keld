# Spec: Host-owned single-instance verdict
Status: draft
Linear: GH-453 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; docs only, do not merge.

## 1. Goal & non-goals

Provide synchronous requestSingleInstanceLock and hasSingleInstanceLock from a host-held cooperative lock and an immutable session
fact, while still launching the losing Bun role for version/help/export paths.

Non-goals: GH524 owns second-instance delivery, additionalData transport and releaseSingleInstanceLock. Protocol registration,
profile leasing and Windows/Linux lock qualification are separate.

## 2. Spec refs

- [Owning issue GH-453](https://github.com/gyldlab/keld/issues/453) and its latest public owner decisions.
- [Architecture
05](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/05-webview-and-native.md):
native ownership, engine boundary and capability separation.
- [Current app
facade](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/packages/@keld/electron/src/app.ts).
- [Boot contract
owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-core/src/app_session/boot_contract.rs).
- [Session facts
draft](https://github.com/gyldlab/keld/blob/c2dd2ff53767adf20dc0e91d276310095fe31acf/docs/specs/gh452-app-session-facts.md).
- [Private-lock owner correction](https://github.com/gyldlab/keld/issues/453#issuecomment-6564005869).
- [Reported kernel-lock observations](https://github.com/gyldlab/keld/issues/453#issuecomment-6560426753).
- [Electron lock contract](https://github.com/electron/electron/blob/694f45852a0f1726cd23bfd379854de489cccb65/docs/api/app.md).

Verified baseline: Current main's facade has no single-instance member. Public comments report local std::fs::File::try_lock
observations and an approved shared-ancestor/private-service correction. This session did not execute that fixture or inspect
unpublished local code.

All proposed API, schema, ownership and permission changes below are future contracts. This documentation PR changes no runtime
boundary and approves none of those changes.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Given a declared app and uncontended stable lock identity, when its host starts and the facade calls before or after ready, then
both calls return true synchronously and hasSingleInstanceLock is true.

Independent negative control: Replace the host-issued fact with a facade-local constant; a second process must detect the lie.

### AC2

Given the same app lock is held, when a second host starts, then its verdict is false but its Bun role still runs and its own
--version path exits successfully.

Independent negative control: Exit the losing host before Bun spawn; assert no version output.

### AC3

Given no compiled declaration, when the member is called, then a registered typed compatibility error names the
declaration/migration fix.

Independent negative control: Return false or true silently on an undeclared app.

### AC4

Given a shared Keld ancestor owned by the user with mode 0755 and no group/other write, when a new private service directory is
admitted, then the ancestor is accepted unchanged; private directories remain 0700 and the lease remains 0600.

Independent negative control: Require every ancestor to be 0700, or accept a writable shared ancestor; test the two mutations
independently.

### AC5

Given symlink, replaced identity, multiple-link lease or wrong private permissions, when admission runs, then it refuses before
role spawn and does not chmod, unlink or truncate the existing path.

Independent negative control: Follow a symlink or silently repair/delete an incumbent lease.

### AC6

Given a live holder and then normal or abrupt holder termination, when a separately synchronized process retries acquisition, then
it observes contention while held and acquisition after kernel release.

Independent negative control: Use existence of a pathname as the lock; the crash-recovery case must fail.

### AC7

Given a successful declaration, when the app delays its first call, then the documented divergence states the lock is already held
from pre-spawn admission.

Independent negative control: Claim Electron call-time acquisition without a test distinguishing pre-spawn contention.

## 4. Design

### 4.1 Identity and custody

Reuse the repository's existing instance-lock owner and approved std::fs::File::try_lock proposal; do not add a lock daemon,
second election service or PID-file protocol. Identity is compiled application identity plus the approved per-user namespace, not
argv or a renderer string. The original Host holds the descriptor for the complete elected lifetime.

### 4.2 Stable namespace

Preserve the public owner correction: a shared ancestor is same-user owned, nofollow/kind/identity stable and not group/other
writable; private InstanceLocks/v1/application-id components are 0700 and the lease is 0600, single-link and close-on-exec.
Perform the owning implementation's pre/post identity checks. Never unlink a held lease: a replacement inode would create a second
election domain.

### 4.3 Verdict delivery

Capture once before Bun spawn and deliver through GH452's existing session-facts contract. Request and query read that fact
synchronously; no blocking RPC or local allocation. Contention is a normal false verdict, while permission/type/identity failures
are typed startup errors, not false success. The losing host still starts the selected app.

### 4.4 Trust limitation

This is cooperative same-user instance election, not protection against a hostile same-user process that can replace its
namespace. Keep that limitation explicit; do not promote election to a filesystem grant or imply Strict-profile isolation. Lock
custody must not leak through child inheritance.

### 4.5 Schema and migration

The compiled boot-schema owner must allocate the declaration against the current schema; the old issue's schema-v1 wording is
historical. No-flag boot must not parse keld.config.ts. Migrate declaration generation and session facts with their existing
owners; the compatibility facade remains permanent. additionalData is only validated/stored until GH524 admits delivery.

### 4.6 Evidence and implementation admission

Use the cited immutable source and re-read live owner claims before implementation. A public issue receipt is reported evidence,
not a test run performed by this drafting session. Reuse current owners and generated contracts; approval of this draft must not
silently approve an upstream draft dependency. Keep each unavailable native or policy prerequisite visible.

## 5. Boundaries

Future implementation stays in the existing Core instance-lock/boot owner, GH452 facts and Electron app facade. Preserve profile
leases, Strict defaults and shared ancestors. No lock-path cleanup or local worktree edits in this documentation batch.

This publication changes only `docs/specs/gh453-host-single-instance-verdict.md` and grouped-PR/issue metadata. Shared policies,
architecture, generated schemas, code and unrelated work remain with their existing owners.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Pin the current compiled declaration/session-fact schema and record pending cells plus the pre-spawn divergence.

### T2

- [ ] Implement stable lock admission and descriptor custody with real two-process contention/release tests and filesystem
substitution negatives.

### T3

- [ ] Thread the captured verdict through GH452 and expose the two synchronous facade methods, including the undeclared typed
error.

### T4

- [ ] Exercise selected original app version/help/export and GUI loser behavior; qualify macOS only and leave other platforms
unknown.

## 7. Test plan

Every AC has a distinct observable and negative mutation in section 3. Map the resulting tests and evidence rows to GH-453;
preserve pending-fail/unknown status until the actual implementing commit passes the relevant fixture.

Synchronize lock holders and contenders through process I/O, not sleeps. Record process lifetime, dev/inode identity and exact
permission refusal. Reproduce the reported kernel fixture independently; a mocked bool or single-process test cannot establish
election.

Documentation validation in this batch covers immutable reference/readback, all ten sections, independent controls,
source-versus-proposal wording and file limits. Runtime, native and original-app tests are planned, not executed. GitHub CI status
belongs to the exact published head; a documentation-only green run does not prove platform behavior.
The user restricted this session to GitHub. Local `just ci`, native execution, hooks and machine session closeout were not run.
Official upstream GitHub sources were used within that source restriction; no Context7 or external-machine evidence is claimed.

## 8. Review gates triggered

Public API and wire/compiled-schema owner review. Permission-boundary review for namespace admission; no new capability is
granted.

Status remains draft. Implementation admission requires the exact owning decisions in section 10 and approval of this contract.
Draft PR publication is not that approval.

## 9. Perf impact

One nonblocking kernel acquisition per host admission and synchronous reads thereafter. Measure only under a pinned fixture; no
startup improvement or hostile-process safety claim.

No numerical product performance claim is made without a reproducible pinned workload, environment and receipt.

## 10. Open questions (block implementation)

1. Approve the compiled declaration and immutable session-facts representation with their owners.
2. Confirm the current private-lock implementation matches the published correction and qualify its descriptor lifecycle.
3. Keep Windows/Linux and hostile same-user replacement outside claimed acceptance until separately specified.

Independent review must refute or fix concrete findings on the published revision. Remaining platform qualification is an
acceptance gap, not evidence of a completed feature.

