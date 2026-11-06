# Spec: Host-owned update triggers and bounded information events
Status: draft
Linear: GH-581 (#517) · Owner: @0monish (approval pending) · Updated: 2026-11-06
Source baseline: `0eb3ce203187f35c5276a4237189c906a5220d5a`
Publication: grouped future implementation pack; docs only, do not merge.

## 1. Goal & non-goals

Specify the future real updater path behind the migrated electron-updater adapter: an authenticated Bun main role may request a
check, download or install while the Rust Host retains update authority. Authority means choosing trusted identity/feed,
verifying signed content, staging, activation, health observation and relaunch. The information returned to Bun cannot convey
filesystem paths or a transferable installation capability. This draft extends coverage under the user's current request; GH-581
implementation stays parked.

Non-goals: implementing the live KEL-53 orchestrator, changing signing roots/feed identity from application code, delta transport, granting
arbitrary network/download authority, a second activation transaction, changing the bridge recipe, implementing Electron-core
autoUpdater, bypassing an unapproved installation mode, launching a legacy installer or privileged package manager from Keld.

## 2. Spec refs

- [GH-581 authority/channel prerequisites](https://github.com/gyldlab/keld/issues/581)
- [X05-T2 channel ownership GH-508](https://github.com/gyldlab/keld/issues/508)
- [KEL-53 exact activation contract](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/specs/kel53-full-package-activation.md)
- [Current update primitives and admission limit](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-update/src/lib.rs)
- [Live-orchestration gaps and platform authority](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/06-runtime-and-tooling.md)
- [Never-grantable update authority](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/docs/architecture/03-security.md)
- [Current guard capability/evaluation owner](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-guard/src/lib.rs)
- [Existing IPC channel table](https://github.com/gyldlab/keld/blob/0eb3ce203187f35c5276a4237189c906a5220d5a/crates/keld-ipc/src/channel_table.rs)
- [Inert adapter specification, independently owned PR1367](https://github.com/gyldlab/keld/blob/a46299fc4b562424aedb175f60bd906144130c5c/docs/specs/gh491-inert-electron-updater-adapter.md)
- [electron-updater6.8.9 AppUpdater](https://github.com/electron-userland/electron-builder/blob/bed3a9c421a8a05c49f36a2af5d7ae3598cb1b95/packages/electron-updater/src/AppUpdater.ts)
- [electron-updater6.8.9 BaseUpdater](https://github.com/electron-userland/electron-builder/blob/bed3a9c421a8a05c49f36a2af5d7ae3598cb1b95/packages/electron-updater/src/BaseUpdater.ts)
- [electron-updater6.8.9 MacUpdater](https://github.com/electron-userland/electron-builder/blob/bed3a9c421a8a05c49f36a2af5d7ae3598cb1b95/packages/electron-updater/src/MacUpdater.ts)
- [Cancelable quit contract in reviewed PR1432](https://github.com/gyldlab/keld/blob/04f65fcfd3cf82fdf1fbf68cee27ab937b4949de/docs/specs/gh451-cancelable-two-phase-quit.md)

Verified source and limits: Main already contains signed v0 manifest/full-content verification, protected installation provenance, Windows staging and
journaled activation/recovery primitives. Its production transaction is admitted for PerUserDirect only. Architecture06
explicitly leaves live-feed orchestration, candidate launch/profile handover, the private health channel with 30-second Ready
observation, release/detach order and installed-host lifecycle composition unimplemented. This proposal must not call the
updater absent wholesale or treat those primitives as a live updater. The current guard includes filesystem/authentication
capabilities; update trigger IDs are still not an approved grant path. The earlier issue's 'only fs.read/fs.write' wording is
stale. The pinned upstream adapter coalesces concurrent checks/downloads, downloadUpdate returns file paths, BaseUpdater
installs then emits on core autoUpdater and quits, and MacUpdater uses native Squirrel with different auto-install behavior.

All proposed API, schema, ownership and permission changes below are future contracts.
This documentation PR changes no runtime boundary and approves none of those changes.

## 3. Acceptance criteria (binary, each becomes a test)

### AC1

Before a real trigger is enabled, approved KEL-53/security text distinguishes grantable trigger intent from never-grantable
update authority; KEL-102 accepts the exact operation set and GH-508 allocates the sole transport contract. Install is named
`update.install` everywhere, with no alias granting a wider operation.

Independent negative control: Add update.install to an app manifest on the present unamended guard or route it through an alternate install ID; admission must
deny before any updater effect.

### AC2

Only the authenticated current main-role identity may invoke a permitted trigger. Host installation provenance,
app/channel/target, signing key and feed selection remain Host-owned; caller arguments cannot alter them.

Independent negative control: Send a renderer/plugin request, stale role epoch, foreign app identity, alternate feed URL or caller-supplied key; observe no
network, staging or activation.

### AC3

Each trigger delegates to the existing Host-owned KEL-53 operation and preserves its identity, signed-size/hash, protected-floor
and mode checks. A check acknowledgment or availability event never means content verification or activation passed.

Independent negative control: Mutate one signed artifact byte or target/channel after selection; no downloaded/staged success or active-version change is
published.

### AC4

`downloadUpdate` on the future real branch resolves without a path. All event, result, error, log-to-Bun and nested
compatibility fields use the approved safe projection; no absolute/relative file path, download URL, staging handle or installer
command crosses to Bun.

Independent negative control: Inject a staging path into a nested error cause, updateInfo.files/path, completion result or event; the schema/projection test
rejects it before transmission.

### AC5

Host coalesces progress to one latest pending value per attempt and keeps terminal transitions independently ordered. Queue
limits include retained bytes/events; saturation never silently drops a terminal result or fabricates completion.

Independent negative control: Flood progress while the consumer stalls, then fail verification. Retained memory stays bounded and the consumer receives an
explicit failure or a defined disconnect requiring authoritative reconciliation.

### AC6

One Host attempt owns one selected update and its concurrent check/download work. Request and attempt IDs cannot replay install,
join an unrelated generation or turn a lost reply into a second activation. Reconnect uses an approved Host summary rather than
trusting Bun's old state.

Independent negative control: Drop the successful install response and repeat the request with an old role/attempt identifier; a second activation cannot
start and the outcome is not relabeled 'no effect'.

### AC7

The information lifecycle distinguishes available, downloading, staged/verified, activating, committed, rolled back and failed
using existing KEL-53 outcomes. `update-downloaded` is emitted only for staged verified content and explicitly does not assert
activation, publication or candidate health.

Independent negative control: Emit update-downloaded after raw download but before integrity/staging validation, or mark a failed candidate committed; state
evidence rejects both.

### AC8

An update-triggered quit composes with the approved GH-451 close transaction and current-window admission fence. Veto/reset,
new/replacement windows and repeated requests preserve the selected stage without falsely claiming install success; no duplicate
core before-quit-for-update signal is invented.

Independent negative control: A close listener vetoes or creates a replacement window. Require the approved live-window/reset outcome and no unapproved
irreversible activation.

### AC9

`autoInstallOnAppQuit=false` followed by Later performs no install on ordinary quit. A request to install without the relaunch
required for Keld health observation is refused or explicitly owner-approved as a divergence; it is never silently treated as
successful install.

Independent negative control: Set autoInstallOnAppQuit=false after staging and close normally, or request no-relaunch in an unqualified mode; active selection
remains unchanged and no candidate is started.

### AC10

A failed health check/activation has the KEL-53 rollback and next-launch behavior, not the adapter's assumption that downloaded
means installed. MachineUacDirect is unavailable until its exact mode/lifecycle proof is admitted, and any UAC prompt is an
explicit compatibility difference.

Independent negative control: Treat a staged package as an active version after health failure, or select a machine mode using only PerUserDirect evidence;
admission fails and rollback receipt identifies the retained selection.

### AC11

The upstream conformance matrix separates BaseUpdater and macOS native paths, callback/event payloads, autoDownload,
autoInstallOnAppQuit, autoRunAppAfterInstall and both quitAndInstall flags. Each of the six recorded divergences in section4.4
has an expected verdict before code.

Independent negative control: Reuse one platform's install/quit ordering for the other, invent release notes, or assert macOS progress from an old caller
comment; the relevant oracle remains unknown/fails rather than passing.

### AC12

The existing inert adapter remains import-pure and refuses unsupported real work until a Host capability/version handshake
admits this branch. Enabling the real branch requires an approved public-type amendment because its info projection and
path-free Promise differ from upstream and PR1367.

Independent negative control: Run the current dev/unqualified packaged app without an admitted updater and require zero feed I/O, no fabricated
checking/downloaded event, and the existing typed refusal instead of partial real behavior.

## 4. Design

### 4.1 Reuse and ownership

Bun owns the compatibility object, listeners and request intent. Rust Host owns the admitted installation, networking, signed
content, attempt lifecycle, private stage paths and all activation/candidate/health authority. Reuse KEL-53's existing
transaction and outcomes; the states in this document are a projection for consumers, not a second journal or activation engine.

Proposed logical operations are update.check, update.download and update.install; these are candidate operation names pending
KEL-102/GH-508 approval, not allocated wire IDs. `update.install` is the single install spelling. A manifest grant authorizes a
trigger only after the new authority split is approved. It cannot mint feed, artifact, signing, target, profile or process
authority. Legacy/same-user role admission and unsupported direct modes retain their existing refusals.

### 4.2 Closed request and safe result projection

Requests contain bounded correlation data and admitted compatibility intent, never URL, key, installer path or executable/argv.
Host binds the authenticated role epoch, installation and selected attempt; it does not accept those identities as self-asserted
caller fields. Install arguments such as silent/relaunch preferences are requests subordinate to approved mode/health
requirements, not a way to suppress required user consent or safety checks.

Proposed safe update metadata contains version, optional release date and validated byte counts. The v0 manifest does not supply
release notes. Events are checking, available, not-available, progress, downloaded and error, with a bounded Host-issued
attempt/correlation identity. Internal transition summaries may name committed/rolled-back/failed outcome categories only
through the approved schema. An attempt ID is correlation, not a transferable capability.

The real adapter needs an explicit public-type amendment: Promise<void> for path-free download completion and a safe updateInfo
shape that omits upstream files/path/url fields. Error projection uses stable codes and bounded public detail; internal paths,
keys, authorization headers and raw nested Error causes remain Host-side. A new type must not masquerade as the existing
path-bearing upstream/PR1367 type. Core GH-582 separately maps events without gaining access to hidden fields.

### 4.3 Correlation, loss and bounded delivery

At most one admitted Host attempt per installation owns mutation. Concurrent equivalent checks/downloads can share that attempt,
mirroring upstream only where the conformance cell agrees; conflicting requests get a typed busy/stale result. Every response
and event is bound to attempt and role generation. Old role events cannot settle the replacement role's Promise. A dropped
response after a stage/activation effect is an unknown delivery outcome, not proof of rollback; retries reconcile through the
same owned attempt.

Progress is latest-value-wins on the Host, with one retained sample rather than an unbounded event history. Terminal transition
delivery has a separate bounded queue/order. If its limit is exceeded, fail/close the observation channel explicitly and require
a newly authenticated authoritative Host summary before further actions. GH-508 must approve whether that summary is part of an
existing response/handshake; this spec allocates no extra endpoint. Decide attempt lifetime across Bun loss under the existing
KEL-53/supervisor contract before implementation; an unqualified replacement role cannot resume or repeat installation.

### 4.4 Platform/conformance decisions

| Cell | Required expected verdict |
| --- | --- |
| update-downloaded | ▲ Staged and verified; activation/health remains separate. |
| failed health / rollback | ▲ Old selection retained or recovered for next launch per KEL-53. |
| install at quit without relaunch | ▲ No headless health boot; unavailable unless separately approved. |
| autoInstallOnAppQuit=false + Later | Install nothing on ordinary quit. |
| MachineUacDirect | ▲ Native UAC may appear; mode proof required first. |
| release notes | ▲ Unavailable from v0 manifest; never fabricated. |
| downloadUpdate return | ▲ No file paths; approved public type change required. |
| upstream BaseUpdater | Install starts before its later core event/app.quit; preserve as oracle. |
| upstream MacUpdater | Native Squirrel path; autoRunAppAfterInstall controls its quit route. |

The Keld design proposes preparing an admitted install, completing the approved cancelable quit boundary and then performing the
irreversible Host transition. If this differs from BaseUpdater's install-before-quit effects, record an explicit ▲ and owner
approval rather than claiming parity. The core facade's shared emitter is handled by GH-582; duplicate adapter/native delivery
cannot multiply events. draw.io's comment that macOS lacks progress is not a current oracle; a pinned macOS run is required.

### 4.5 Failure, rollback and seam with the inert adapter

Until prerequisites land, PR1367's inert adapter stays the current behavior. Do not use application `setFeedURL`, logger
options, local booleans or app.isPackaged alone to enable real work. A versioned Host admission fact must prove the correct
supported contract and installation mode. Unsupported mode, trust failure or missing grant refuses before effects.

Once a mutation crosses its committed-effect boundary, diagnostics distinguish failed observation, recoverable attempt, rollback
and committed result. Never auto-retry install on a rejected Promise alone. Journal/candidate cleanup belongs to KEL-53; facade
listener disposal cannot delete a protected stage or installation. Tests must observe native selected version/process lifetime
where relevant, not only event emission.

## 5. Boundaries

Allowed future implementation surfaces: coordinated Host orchestration seam, owned keld-update/guard/channel changes after their
separate prerequisite approvals, and the existing adapter package. This docs-only PR edits none. KEL-53, GH-508, KEL-102, GH-451
and PR1367 retain their owners. Exact operation IDs/wire fields, error codes, trigger grant scopes, event byte/count limits,
retry/cancellation and role-loss policy require approval. No updater authority is delegated to app permissions and no lifecycle
event transport is borrowed without channel allocation.

## 6. Tasks (ordered, each independently reviewable)

### T1

- [ ] Publish source-pinned platform conformance entries and approve the KEL-53 trigger/authority amendment, exact guard IDs/channel
allocation and path-free adapter type change. Resolve all section10 gates before enabling code.

### T2

- [ ] Implement the Host admission/delegation seam over the existing updater transaction; verify forged role/identity/feed/grant,
wrong mode, integrity failure and duplicate attempt controls before adding facade calls.

### T3

- [ ] Add safe information projection, bounded progress/terminal delivery and generation-aware correlation to the existing adapter.
Preserve inert fallback, listener semantics and import purity; prove absence of paths recursively.

### T4

- [ ] Integrate quit/veto, real candidate health/rollback and per-platform flags on admitted native cells. Record actual
stage/selection/process outcomes, disconnect/retry behavior and all divergences; release only against those receipts.

## 7. Test plan

Begin with primary source traces for AppUpdater, BaseUpdater and MacUpdater at the pinned electron-updater6.8.9 commit. Register
the exact package artifact/digest in the future executable oracle; repository source is not proof that an installed npm artifact
ran. Guard tests independently count attempted network/staging/activation effects; projection tests inspect encoded bytes and
nested errors. Transport tests stall consumers, reorder/stale events and drop replies at each effect boundary. Native tests use
the owned KEL-53 fixture installation, real candidate/health receipts and before/after selected versions. Exercise Later, close
veto, replacement windows, unsupported no-relaunch, ordinary quit, UAC cancellation and health rollback separately. Nothing in
this draft ran a feed or activated an update.

Every acceptance criterion has an independently observable outcome and named negative control.
Record source commit, pinned reference, exact platform/runtime and raw receipt for every claimed result.
Documentation validation covers immutable source/readback, ten sections, controls and file limits.
Runtime, native and original-app tests are planned, not executed by this drafting session.
GitHub documentation CI cannot establish platform acceptance. Local `just ci`, hooks and machine
closeout were not run: the user restricted this work to GitHub. Official upstream GitHub sources
were read within that restriction; no Context7 query or external-machine evidence is claimed.

## 8. Review gates triggered

Permission model, wire protocol and public API are mandatory gates. Security/activation reviewers own trigger authority,
identity/provenance, private stages, mode admission and effect/rollback boundaries. Native process/installation proof is
mandatory for candidate lifecycle and machine elevation. Unsafe/dependency gates depend on later implementation changes. PR1367
and KEL-53 cannot be silently amended by this proposed contract; source research and docs CI satisfy no native gate.

Status remains draft. Implementation requires the section 10 decisions and approval of the exact contract.
Draft publication does not approve an upstream dependency or change a parked implementation schedule.

## 9. Perf impact

No change runs in this documentation batch. The future design adds bounded metadata frames and one pending progress sample per
active attempt. It must not duplicate package buffers, signed-content verification or activation journals in Bun. Measure Host
memory, retained event bytes, progress frame rate and time to terminal delivery under a stalled consumer; numeric bounds and
coalescing interval remain approval inputs. Download/verification cost stays with the existing streaming KEL-53 implementation.

## 10. Open questions (block implementation)

1. KEL-53/security owner: approve trigger-versus-authority text and admitted installation modes; provide the
live-feed/candidate/health composition task and exact terminal proof before wiring real triggers.

2. KEL-102/GH-508 owners: approve the three logical operation IDs, grant scopes, wire/channel version, correlation identity,
event/result schema, numeric queue limits and authoritative reconnect summary.

3. Adapter/public-API owner: approve path-free Promise/result types and each divergence while preserving PR1367's current inert
contract until Host admission exists.

4. Lifecycle owner: approve the install/quit irreversible boundary, close-veto behavior, shared core emitter and
no-relaunch/auto-install policy. There is no headless-health implementation to assume.

5. Runtime/activation owner: define attempt continuation/cancellation on Bun loss, Host loss, reconnect and repeated install;
use KEL-53 recovery rather than adding a parallel journal.

6. Oracle owner: provide the exact electron-updater6.8.9 platform package/source receipt, macOS progress observation and native
quit/flag/rollback traces. No source-only trace is a native PASS.

Independent review must refute or fix concrete findings on the published revision. Platform gaps
remain explicit; no unexecuted acceptance test is reported as passed.
