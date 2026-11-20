# Managed service design decisions

Part of: [index](index.md)
Owns: Runtime ownership, admission, durable outcomes and architectural alternatives.

## 4. Design

### D-1: Extend canonical ownership instead of nesting managers

FACT: `RoleOwner` provides Primary and AppBound. Both stop with the host application session.
References: `crates/keld-runtime/src/role.rs:248` and `crates/keld-runtime/src/registry.rs:187`.
FACT: `RoleRecoveryGate` controls role restart after application readiness.
Reference: `crates/keld-runtime/src/role.rs:172`.
INFERRED: That gate does not itself authorize an OpenCode database sweep or prove descendant termination.

FACT: Upstream client management signals a prior process and waits for its PID to stop.
Source: [service.ts](https://github.com/anomalyco/opencode/blob/4423f42836aaaf2933bc7b94b35aca4e5bcce7c3/packages/client/src/promise/service.ts#L153-L165).
FACT: The core states, "Recovery is at-least-once" and assumes previous claim owners are dead.
Source: [restart.ts](https://github.com/anomalyco/opencode/blob/4423f42836aaaf2933bc7b94b35aca4e5bcce7c3/packages/core/src/session/execution/restart.ts#L47-L62).

PROPOSED: The canonical runtime owner controls the installed service's activation, custody, replacement and Stop.
That owner delegates recovery admission to the existing core only after custody and data-root checks agree.
The adapter replaces upstream managed recruitment and termination at this boundary.
It must not retain a second management loop for the same managed service.
Independent external services keep their own owner and a distinct management mode.
Connecting to an external service cannot silently adopt it.

| Alternative | Disposition and decisive reason |
| --- | --- |
| Canonical installed ownership | Recommended, conditional on B-1/B-2. Reuses runtime policy while preserving independent background work. |
| Fully external management | Credible fallback. Must include clean-machine provisioning, update, recovery and removal under one identified owner. |
| Desktop AppBound backend | Reject for the full denominator. Its lifetime ends with the desktop application session. |
| Unchanged upstream manager nested below KELD | Reject. Independent replacement and recovery decisions can create competing owners. |
| SDK inside privileged desktop host | Reject as the default. It couples crash/lifetime domains without solving independent recovery. |

No alternative wins on unmeasured startup, memory or implementation cost.
Reopen this decision if canonical ownership cannot satisfy native custody with acceptable maintenance cost.
D-5 selects the first platform candidates. Q-1 retains exact installation and custody approval.

### D-2: Bind authority to installed identity and generations

PROPOSED: The canonical owner derives installed identity, data-root identity and admitted artifact identity.
Untrusted paths, endpoint strings, saved PIDs and renderer identifiers are not authority.
The authorization owner distinguishes Connect, Use and each management operation.
Management includes activation, adoption, Stop, replacement, update and recovery admission.

The owner captures effective configuration and grants before admission.
The effect owner rechecks revocation and generation before dispatch.
Changed command inputs require new authorization at the existing effect boundary.
The runtime owns process identities and descendant cleanup.
The state owner owns writer admission, schema compatibility and committed outcomes.
The bridge owns client-binding generation checks and delivery retirement.
The updater owns package selection and compatibility under #1385.
It requests activation from the runtime owner.

These owners exchange decisions. They do not duplicate each other's policy.
ServiceIdentity names installation, account and data root. ServiceBinding additionally names mode, artifact and owner generation.
Mode-specific authorization binds ServiceBinding. The narrower account/root arbitration key does not grant application or management authority.
Adoption requires an authenticated instance witness and an approved custody transfer.
Bearer credentials alone do not authorize adoption or management.

Types, manifest identifiers, wire fields and Rust signatures: none allocated by this draft.
D-6 selects the semantic contract. Q-2 retains exact typed bindings before implementation.
D-9 provides proposed typed bindings. Shared endpoint allocation and owner approval remain pending.
The public API, permission and wire owners review that contract together.

### D-3: Admit recovery only after physical or effective exclusion

PROPOSED transition order:

1. Authorize the management request and bind its inputs.
2. Serialize activation for the installed identity and data root.
3. Retire conflicting client and effect bindings.
4. Prove predecessor custody ended or enforceably excludes all conflicting effects.
5. Admit the selected artifact and writer generation under the state contract.
6. Classify unfinished outcomes before invoking the core recovery entry point.
7. Publish the new service binding only after readiness succeeds.

Each fallible step preserves a recoverable owner record.
A failed activation never restores stale credentials or silently replays uncertain work.
Retries can discover the already-admitted result using the same operation identity.
An admission timeout returns pending or indeterminate status when effects may have occurred.

Native-trusted code can retain ambient process and database access.
Logical generation revocation cannot fence those effects by itself.
For that profile, replacement requires verified termination of all conflicting custody.
For a constrained profile, demonstrate effective fencing at every affected effect owner.
Incomplete fencing blocks replacement. It cannot become a profile downgrade during acceptance.

Windows retained handles and Job/ConPTY custody require Windows evidence.
Unix process and PTY custody require separate macOS and Linux evidence.
UNKNOWN: The current proposal has no qualified installed activation mechanism on all three systems.
Keep that limitation in B-2 and U-1.

### D-4: Preserve outcomes across cancellation and replacement

PROPOSED outcome classes: not admitted, pending, succeeded, failed, cancelled with partial effects, and indeterminate.
Cancellation is a request. An acknowledgement must identify which effects remain possible.
Preserve a successful result if cancellation arrives after its commit.
Do not infer no effect from connection loss, process loss or a response timeout.

The core retains session and operation semantics.
The adapter carries outcome identity and admission evidence across the managed boundary.
It does not implement a second job journal.
D-7 selects outcome and retention semantics. Q-3 retains exact storage admission before approval.
Neither exclusive ownership nor SQLite transaction atomicity proves exactly-once external effects.

Migration includes recruitment, discovery, Stop, replacement, startup recovery, writer admission and bridge retirement.
Keep the permanent compatibility facade for upstream callers.
Remove temporary interception only after all managed callers use the canonical owner.
Required external and remote connection flows remain distinct and functional.

FACT: Pinned recruitment carries terminal handoff state. Reuse completes the handoff, and Stop prepares or clears it.
Source: [service lifecycle](https://github.com/anomalyco/opencode/blob/4423f42836aaaf2933bc7b94b35aca4e5bcce7c3/packages/client/src/promise/service.ts#L40-L110).
FACT: Early desktop adoption still invokes ensure for "terminal handoff completion".
Source: [early adoption](https://github.com/anomalyco/opencode/blob/4423f42836aaaf2933bc7b94b35aca4e5bcce7c3/packages/desktop/src/main/service/background-service.ts#L81-L86).
PROPOSED: Transfer prepare, ticket transport, completion, expiry and failed-replacement cleanup through the existing #1382/KEL-76 owner.
Early compatible discovery must still complete pending handoff. Preserve explicit best-effort failure behavior without guaranteeing unproved terminal survival.
Do not create a second PTY controller or let an environment ticket confer native custody.

### D-5: Select per-user activation candidates, not unproved custody

PROPOSED: Use the following first qualification candidates under D-1. B-1 and B-2 remain open.
D-8 supplies the proposed registration settings, authenticated client paths and lifecycle ordering.
Linux uses owner-launched native session hosts and independent client scopes as its first authentication candidate.
Retained enrollment requires the specified service-manager support. An unsupported configuration cannot substitute UID or bearer possession.

| Platform | Selected candidate | Required qualification |
| --- | --- | --- |
| macOS 13 or later | SMAppService per-user LaunchAgent with an authenticated XPC/Mach endpoint. | Approval and disabled states, independent lifetime, detached descendants, update and removal. |
| Windows | On-demand Task Scheduler task with InteractiveToken, LUA and IgnoreNew. | Interactive-session identity, task settings, Job membership, retained handles and actual cleanup. |
| Qualified Linux | systemd user service with Type=notify, NotifyAccess=main and KillMode=control-group. | Actual cgroup membership, user-manager availability, owner loss, logout and native escape paths. |

The OS activates the root. The existing Supervisor owns enrolled resources. The canonical guard owns application authorization.
Stable identity includes the installed component, account and canonical physical data root. Artifact identity and owner generation remain separate.
Two installed copies targeting one root must share exclusive management and writer admission.
Key that arbitration by account and canonical physical data root, not installation, artifact version or service mode.
Keep installation and artifact checks in authorization and compatibility admission. An incompatible losing candidate cannot adopt the active service.
Same-account OS administration is an ambient platform boundary, not KELD application authorization.

Require explicit registration. Preserve user disablement. Refuse unavailable activation without a detached fallback.
Use no stored password, default machine privilege, automatic OS crash restart, or default Linux lingering.
Windows registration must pin PT0S, battery and idle settings for required long-running work.
Every activated root begins non-effectful until custody, writer and recovery admission succeed.
Logout, reboot and forced OS Stop are interruption cases. This proposal does not add survival guarantees for those events.

Persist Stop intent before draining. Only a new authorized Start can admit work after that Stop.
Complete graceful custody checks before disabling registration through the package owner.
Forced unregister or OS termination is not graceful cleanup evidence. Outstanding remote effects retain their outcome records.
The updater owns artifact selection and compatibility. Runtime owns admission of the replacement generation.
Retain live artifacts until their owners release them. Removal cannot stop external services or erase user state implicitly.

FACT: Apple describes registration as "subject to user approval".
Source: [SMAppService](https://developer.apple.com/documentation/servicemanagement/smappservice). Retrieved 2026-11-16.
FACT: Apple unregister terminates a running agent: "the system terminates it".
Source: [unregister](https://developer.apple.com/documentation/servicemanagement/smappservice/unregister()). Retrieved 2026-11-16.
FACT: Microsoft permits low-privilege registration using "TASK_RUNLEVEL_LUA".
Source: [task security](https://learn.microsoft.com/en-us/windows/win32/taskschd/security-contexts-for-running-tasks). Retrieved 2026-11-16.
FACT: IgnoreNew applies when "an existing instance of the task is running".
Source: [task instance policy](https://learn.microsoft.com/en-us/windows/win32/taskschd/taskschedulerschema-multipleinstancespolicy-settingstype-element). Retrieved 2026-11-16.
FACT: systemd control-group termination covers "the control group of this unit".
Source: [systemd.kill](https://www.freedesktop.org/software/systemd/man/262/systemd.kill.html). Retrieved 2026-11-16.

INFERRED: These activation primitives do not prove complete native-trusted descendant custody. B-2 retains that requirement.
Client-bound helpers fail independent lifetime. Machine-privileged services add unjustified authority. Full external management remains a credible alternative.
Reopen D-5 if native custody, required sharing, platform support or package lifecycle cannot satisfy the complete denominator.

### D-6: Select canonical identity, dispatch and bounded failure semantics

PROPOSED: Extend canonical runtime, guard and IPC owners. Do not add an adapter permission database or credential authority.
The following semantic records do not allocate public signatures, wire IDs or manifest fields. Q-2 gates those exact bindings.

```text
ServiceIdentity = installed component + account + canonical physical data-root identity
ServiceBinding = ServiceIdentity + declared mode + artifact digest + fresh owner generation
ClientBinding = authenticated app principal + client generation + current grant revision
OperationRef = state-owner-issued replay domain + increasing operation sequence
Admission = OperationRef + upstream identity + actor + action + target + input digest
EffectBinding = Admission + current owner/effect/writer generations + admitted grant revision
```

Keep ManagedLocal, ExternalLocal, SSH remote and owned WSL modes distinct. Connecting never silently adopts management.
Authenticate the OS peer and application separately. Same UID, bearer secret, endpoint, path and PID are not sufficient application authority.
Keep canonical credentials outside renderers, URLs, redirects and telemetry. Q-2 must bind persistent endpoint authentication before approval.
Guard Connect, Use, QueryOutcome, Cancel and each management operation separately. Outcome queries require current authorization.
Bind arguments, target, environment and secret versions, plugin revision and canonical resources before approval and dispatch.
Preserve operation identity across service replacement. Never include volatile client generation in the stable retry key.
Return InputConflict for reused identity with changed inputs. Unknown or expired references cannot silently become AdmitNew.
First admission uses an atomic create-or-read operation. A concurrent insert returns the existing bound record or a typed conflict.

Reserve bytes and pending-call slots before admission. Recheck current grants and retirement at the actual effect owner.
Serialize dispatch admission with revocation. Atomically compare Prepared state, bound inputs, and current owner and writer admission.
Commit dispatch intent before release. Only the successful claimant may release that attempt. Do not hold locks during external work.
Revocation before admission denies dispatch. After admission, cancellation reports possible effects instead of inventing rollback.
Persist verified outcomes before durable success. Deliver only to a live authorized binding. Retired delivery does not erase committed outcomes.
Every outcome transition compares current state and version. Recovery and cancellation cannot downgrade a committed success or delete its receipt.

| Prototype budget | Initial bound, subject to full-journey load validation |
| --- | --- |
| Pending RPCs | 8 per client, 64 per service. Long-lived resource leases are separate. |
| Ordinary buffered data | 256 KiB per client, 4 MiB per service. |
| Request, reply, stream chunk | 32 KiB, 64 KiB, 32 KiB. Larger payloads need the approved bounded streaming contract. |
| Stream credit | 256 KiB per client, 2 MiB per service. |
| Control reserve | 8 entries and 256 KiB, plus an independent Stop/revoke execution permit. |
| Total user-space buffers | 8 MiB, including partial messages, responses and intermediate copies. The lower applicable bound wins. |
| Connect, arbitration, readiness observation | 5 seconds, 10 seconds, 30 seconds. |
| UI wait, graceful drain, cleanup observation | 30 seconds, 5 seconds, 15 seconds. |
| Safe observation retry | At most two. Deterministic denial has zero retries. Effectful retry needs proven non-admission or certified deduplication. |

These values are unmeasured prototype limits, not shipping performance claims or permission to drop required capabilities.
Overload before admission returns NotAdmitted(Capacity). An admitted timeout returns Pending or Indeterminate, never false non-admission.
Enforce ingress bounds before decoding where possible. Host accounting cannot retroactively prevent native engine allocation.
Backpressure must preserve required terminal replay and fidelity. No silent truncation, unbounded spool or control starvation is permitted.
Reopen limits or transport if full journeys cannot satisfy them.
D-9 specifies initial attachment, handshake and resource limits. Native measurement remains a qualification gate under U-1.

### D-7: Select durable intent and explicit reconciliation in the existing core

PROPOSED: Extend the existing core state owner, not an adapter-owned second journal. Preserve session, prompt and schedule identity.
Prepared records admission. DispatchIntent records possible release. Known outcomes preserve success, failure and partial effects.
An irreversible effect without verified completion remains Indeterminate. Local process death cannot establish remote completion.
Certified retries reuse the original provider account, target, payload and key within the provider's actual retention guarantee.
Otherwise require authoritative reconciliation or deliberate new admission linked to the unresolved operation.
Hold automatic effectful continuation of the affected execution while irreversible outcomes remain unknown.
A fresh tool-call ID cannot evade that hold. Authorized inspection and unrelated independent work remain available.
This changes automatic at-least-once continuation explicitly. Approval must preserve all required journeys without silent feature reduction.

Use WAL/FULL on every writer of the authoritative admission/outcome store. Verify the applied settings.
Separate admission acknowledgement, process-crash commit and a qualified power-durable acknowledgement class.
Qualify native VFS, filesystem and macOS fullfsync separately before claiming power durability. Cache policy can remain weaker.
FACT: SQLite WAL/FULL adds synchronization "after each transaction commit".
Source: [SQLite synchronous](https://www.sqlite.org/pragma.html#pragma_synchronous). Retrieved 2026-11-16.
No synchronization setting proves external-effect completion. Disk loss and malicious storage replacement remain outside this proposed failure model.

Retain unresolved outcomes. Compact completed records only with durable replay-domain high-water marks and rejection floors.
Never advance a contiguous floor past unresolved holes. Expired or unknown references refuse dispatch.
AdmitNew requires deliberate current authorization. Reconnect and retransmission cannot silently mint new effect attempts.
Prototype retention is 30 days for detailed completed outcomes, 128 active domains and 1024 unresolved operations per data root.
D-10 specifies a logical byte quota and reserved completion allowance. Physical WAL and filesystem capacity require separate qualification.
At quota, refuse new effects while preserving outcome persistence, inspection, reconciliation and cleanup.

Writer checks and writes share an admission transaction. Native-trusted direct writers require actual custody exclusion, not cooperative epoch promises.
Binary rollback preserves outcome history and rejection floors. Never automatically overwrite newer user work with an old data snapshot.
Approved restore retires old replay domains and requires reconciliation before automatic effects.
An absent record in an old snapshot does not prove that an external effect never occurred.
Schema changes require existing storage/update approval. Pending migrations cannot claim the current dataMigration:none policy.
FACT: The approved updater slice exposes no migration hook: `docs/specs/kel53-full-package-activation.md:606`.
Reopen D-7 if required effects bypass instrumentation, expired retries become executable, or full journeys cannot preserve explicit uncertainty.
