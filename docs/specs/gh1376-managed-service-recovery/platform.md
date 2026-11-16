# Platform registration and custody contract

Part of: [index](index.md)
Owns: D-8 platform registration, authentication, client lifetime and native custody candidates.

## 4. Design

### D-8: Installed activation, authenticated clients and native custody

PROPOSED: Extend D-5 with the registration and lifecycle contract below.
Source observations use `413c128e5375df3ffa241809db651bf7fd27e498` unless another source is cited.
Q-1 remains open for its exact peer, CLI authority, profile and lifecycle bindings.
UNKNOWN: Complete native-trusted descendant custody remains unresolved. B-2 remains open.
This candidate authorizes no implementation or native qualification.

#### Atomic inputs before selection

| Atom | Owner and boundary | Input and output | First independent falsifier |
| --- | --- | --- | --- |
| Registration | Package owner to OS activator | Verified installed component and account to registration state | Disabled registration still creates an unmanaged root. |
| Authentication | Platform adapter and guard | Connected OS peer plus admitted artifact to private client binding | An unapproved same-account executable obtains that binding. |
| Root arbitration | Runtime to physical data root | Retained directory identity to one management lease | Distinct installations admit two managers for one physical root. |
| Custody | Runtime and process owner | Enrolled effect family to retained custody evidence | A detached descendant remains effectful after claimed cleanup. |
| Lifecycle | Runtime and state owner | Authorized Start or Stop to durable desired state and observed result | A late Start or OS wake undoes a committed Stop. |
| Retirement | Updater and runtime | Selected candidate plus retired bindings to replacement admission | Old artifacts regain admission or live stages are removed. |

Registration does not establish authentication, custody or writer admission.
The root lease does not establish predecessor death or external-effect completion.
Authentication does not grant management authority. Guard decisions remain operation-specific.
Retirement depends on custody and state evidence. It does not mint either result.

#### Existing owners and missing behavior

FACT: `RoleOwner::Primary` and `RoleOwner::AppBound` stop with the host application session.
Source: `crates/keld-runtime/src/role.rs:248@413c128e5375df3ffa241809db651bf7fd27e498`.
FACT: The current Mac Guardian cannot contain detached descendants or simultaneous host-and-Guardian death.
Source: `docs/architecture/06-runtime-and-tooling.md:220@413c128e5375df3ffa241809db651bf7fd27e498`.
FACT: Existing Windows peer observation requires separate installed-image authentication and a live protocol challenge.
Source: `crates/keld-runtime/src/windows_job/process_peer.rs:20@413c128e5375df3ffa241809db651bf7fd27e498`.
PROPOSED: Extend the canonical runtime with an OS-activated root session. Keep the existing Supervisor beneath that root.
The desktop owns an attachment to that session. Desktop exit releases that attachment and its desktop resources.
Do not add an adapter supervisor, service journal, package verifier or independent terminal owner.

#### Registration contract

PROPOSED: The package owner supplies the registration identifier, executable, fixed arguments and artifact admission record.
Use one registration per installed component and account. Derive registration bytes from the verified installation.
Do not accept executable paths, command arguments, service labels or credentials from application payloads.
Read back effective registration settings. Refuse substituted paths, principals, arguments, activation policies or signing identities.
Registration state is `Enabled`, `Disabled`, `RequiresApproval`, `Absent`, `Conflicting` or `Unverifiable`.
Only `Enabled` permits activation. The other states return their typed refusal without detached fallback.
Do not enable a disabled registration automatically. An explicit registration action must retain its applicable authority and OS consent.
OS activation starts a non-effectful root. It does not constitute an authorized Start.
Persisted Stop, authentication, management arbitration, custody and writer admission all precede effectful Ready.
Treat logout, reboot and forced OS termination as interruption. Do not promise work continues through those events.

| Platform candidate | Exact proposed registration settings | Qualification boundary |
| --- | --- | --- |
| macOS 13+ | `SMAppService.agent(plistName:)`. Place the plist under `Contents/Library/LaunchAgents`. Use bundle-relative `BundleProgram`. Declare the package-owned Mach service. Set `RunAtLoad=false`, `KeepAlive=false`, `AbandonProcessGroup=false`. | Validate status, user disablement, bundle replacement, signing requirements and Mach lookup. Group cleanup remains incomplete custody. |
| Windows | Register through Task Scheduler under the existing interactive user. Use `TASK_LOGON_INTERACTIVE_TOKEN`, `TASK_RUNLEVEL_LUA`, `IgnoreNew`, `AllowStartOnDemand=true`, `ExecutionTimeLimit=PT0S`. | Verify exact user SID, interactive session, registered executable and every effective setting. The task policy controls only its registration. |
| Linux | Use the qualified `systemd --user` manager. Set `Type=notify`, `NotifyAccess=main`, `Restart=no`, `KillMode=control-group`, `SendSIGKILL=yes`, `Delegate=no`. | Verify the actual cgroup, manager identity, unit contents and supported kernel features. A user unit is not an application identity. |

For Windows, set `DisallowStartIfOnBatteries=false`, `StopIfGoingOnBatteries=false`, and `RunOnlyIfIdle=false`.
Omit triggers, `RestartOnFailure`, idle conditions and maintenance settings. Preserve explicit user disablement.
Use no stored password, highest-privilege task, LocalSystem account or default elevation.
For Linux, use the package-owned executable without a shell. Do not enable lingering or rewrite other units.
Treat service-manager Ready as an activation observation. D-9's RootReady describes an authenticated non-effectful root.
Publish Running only after the required custody, writer, control and recovery checks succeed.
Use D-6's existing observation deadlines. An expired observation produces `Pending` or `Unverifiable`, never fabricated cleanup success.
These candidates do not change the supported platform denominator. Unsupported cells remain unqualified until a complete alternative exists.

#### Local authentication contract

PROPOSED: Only admitted native KELD hosts attach directly. Bun, renderer, plugin and tool code use their existing guarded host routes.
Both peers authenticate the other before exchanging application credentials or effectful requests.
For macOS, the first outgoing request carries only a bounded public challenge and protocol version.
Wait for a validated response on that connection before sending credentials, user data or effectful payloads.
Re-establish that proof after interruption or replacement. An earlier connection's response cannot authorize the new connection.
The platform adapter derives account, process instance and executable evidence from the connected native transport.
The package verifier binds that evidence to a currently admitted installation, executable role and artifact generation.
Runtime mints D-9's private client context from the verified evidence. Guard evaluates each separate action grant.
Keep native peer evidence and process handles private. No wire value constructs a principal or verified peer.
Use D-9's bounded fresh challenge to bind the live connection and both artifact generations.
Challenge success cannot override failed native identity, image, role, generation or grant checks.
Keep authority channels non-inheritable. Never transfer them to application roles or tool descendants.
Retire the client attachment on client exit, connection replacement, artifact retirement or failed identity revalidation.
Attachment retirement does not revoke separately admitted execution authority. D-9 retains its actor, target, grant and lifecycle checks.

| Platform | Proposed authentication mechanism | Remaining obligation |
| --- | --- | --- |
| macOS | Set reciprocal XPC code-signing requirements before resuming either connection. Derive requirements from the package's admitted signing identity and exact executable identity. | A matching Team ID alone cannot admit another executable or a retired artifact. Test unsigned, wrong-role, wrong-artifact and retired peers. |
| Windows | Reuse `WindowsProcessPeer`, canonical token facts, pinned installed-image verification and a live challenge. Retain both connected process objects. | Prove connection-to-object binding for independently OS-started roots. Existing launch-object comparison does not automatically provide this binding. |
| Linux | Select root-spawned native session hosts with retained process identity and private one-shot child links. | Give each client an independent lifetime before admission. Qualify reconnect after root loss. |

FACT: Apple provides `NSXPCConnection.setCodeSigningRequirement` from macOS 13.
Source: [Apple XPC peer validation](https://developer.apple.com/forums/thread/681053), retrieved 2026-11-16.
FACT: XPC code-signing checks apply to received messages. An outgoing first message can reach an untrusted peer.
Source: [Apple DTS outgoing-message clarification](https://developer.apple.com/forums/thread/837286), retrieved 2026-11-16.
Apple explains that validation must run "on a received message". Setting a requirement is not an outgoing disclosure barrier.
FACT: Linux `SO_PEERCRED` returns credentials captured when the connection was established.
Source: [Linux man-pages 6.15, UNIX(7)](https://www.kernel.org/pub/linux/docs/man-pages/book/man-pages-6.15.pdf), retrieved 2026-11-16.
INFERRED: Executable identity requires an explicit launch-context threat model. A file digest does not prove the process excludes injected code.
Do not claim hostile same-account native-code protection from these checks. Do not classify a native-trusted tool as a constrained role.
No missing API implementation alone defeats this proposal. Missing authentication properties prevent the corresponding binding from being approved.

#### Selected Linux enrollment candidate

PROPOSED: Let the installed service root launch native desktop and CLI session hosts through the canonical runtime owner.
An untrusted activation request can request only a bounded, package-declared session kind. It grants no Use or management authority.
The root selects the verified host image, fixed bootstrap arguments and canonical environment. It mints one private child link.
Reuse the one-successful-accept bootstrap and retained generation owner. Do not expose a shared bearer-token endpoint.
Bind the host principal to the root's retained spawned object. Reject a supplied PID, pathname, executable or identity claim.
Keep the child non-effectful until its independent lifetime and authenticated link are established.
Create a separate transient user scope through `StartTransientUnit`, using the retained child's `PIDFDs` property.
Read back its unit and invocation through `GetUnitByPIDFD`. Require the expected scope outside the backend's cleanup cgroup.
Do not fall back to numeric `PIDs`. Failure retains cleanup responsibility and refuses session admission.
The client owns its desktop resources. Backend Stop and root death cannot silently terminate the desktop or its own helpers.
Preserve CLI input, output, exit status, TTY resize, signals and backpressure through the owning terminal contract.
Never let launch requests supply loader variables, inherited authority descriptors or arbitrary native commands.
Forward user launch context as untrusted data. The admitted host applies the ordinary action guards before any effect.
An unauthenticated launch request cannot authorize a forwarded CLI command. Qualify the full CLI authority handoff separately.
Existing clients retain their windows during reconnect. Retire their old binding before issuing a fresh binding.
PROPOSED: Retain each enrollment through systemd's descriptor store, separate from the ordinary application stream.
Store exactly one private `SOCK_SEQPACKET` control endpoint, one retained client pidfd and one sealed metadata memfd per enrollment.
Set `FileDescriptorStorePreserve=yes` and `FileDescriptorStoreMax=96`. Permit at most 32 enrolled hosts per root and eight authenticated clients per service.
Charge pending enrollments against the root limit. Share four concurrent pre-authentication and rebind slots per root.
Each handshake has D-9's five-second absolute deadline and 4 KiB envelope. Charge retained bytes to D-9's existing budget.
Metadata is at most 4 KiB. Require `F_SEAL_WRITE`, `F_SEAL_GROW`, `F_SEAL_SHRINK` and `F_SEAL_SEAL` before submission.
Bind its enrollment nonce, creation generation, account, admitted artifact, session kind, scope invocation and both retained descriptor identities.
Use distinct names `keld-enroll-<nonce>-control`, `-process` and `-record`. Names contain no credentials and grant no authority.
Submit control and process separately with `FDSTORE=1`. Use a notification barrier and `DumpFileDescriptorStore` readback before submitting the sealed record last.
Read back the complete tuple before admitting the session. Notification success and barrier completion alone do not prove storage succeeded.
On restart, require exactly three coherent descriptors, valid seals and matching native identities.
Require a live client and currently admitted artifacts before rebinding.
Reject missing, duplicate, mixed-generation or retired tuples. Remove their remnants before releasing capacity. Never reconstruct them from names or numeric PIDs.
Poll the retained pidfd for client exit. Retire the entire tuple after exit or permanent disconnect.
Keep each inherited control endpoint private. A fresh bounded challenge creates a new one-successful-accept app link for the new root generation.
Open a new application stream. Never resume a lost stream decoder or dispatch effect requests through the enrollment control channel.
Each rebind re-evaluates D-9 grants and the current control revision. Stored enrollment never restores an old grant.
Stop closes effect release before its durable commit and drain. Retained enrollment permits inspection and fresh management authorization only.
Before removal, persist the removal hold and revoke bindings. Remove all tuple names with `FDSTOREREMOVE=1`.
Verify their absence before unregistering.
Use the existing state/update owner for that hold. A crash during retirement cannot restore effects from retained descriptors.
Manager loss, incomplete descriptor transfer or unsupported native primitives leaves the affected client unattached.
Qualify graceful handoff, abrupt root loss and tuple-assembly crash cuts without silently restarting surviving GUI hosts.
This mechanism retains enrollment only. It proves neither OS-boot continuity nor database lineage, external outcomes or power durability.
Keep independent Unix-client authentication as an alternative, not an assumed fallback. It needs separate launch-context and peer proofs.
FACT: systemd v262 accepts `PIDFDs` when creating scopes and exposes `GetUnitByPIDFD`.
Sources: [scope implementation](https://github.com/systemd/systemd/blob/v262/src/core/dbus-scope.c#L143-L171), [manager API](https://www.freedesktop.org/software/systemd/man/262/org.freedesktop.systemd1.html), retrieved 2026-11-16.
FACT: The v262 descriptor store accepts valid descriptors without a socket-only filter. Non-pollable descriptors remain accepted.
Source: [descriptor store implementation](https://github.com/systemd/systemd/blob/v262/src/core/service.c#L662-L714), retrieved 2026-11-16.
INFERRED: A seqpacket socket, pidfd and sealed memfd can use this API. Native tests must qualify retention, process exit and removal for all three.

#### Root arbitration and custody contract

PROPOSED: Resolve the data directory through the existing guard-owned path rules and retain its native identity.
Acquire one exclusive management lease beneath that retained root. Distinct installations must contend on the same root object.
Reject unsupported root identity, reparse substitutions and unsupported filesystem locking semantics.
Keep installation and artifact identities in authentication. Do not place those identities in the exclusive root-arbitration key.
Hold the lease through the root's final custody result. Do not replace the lease inode or rename its anchor during ownership.
Use the existing core owner for writer admission. A management lease never grants database access.
Do not launch a conflicting successor merely because the root lease became obtainable.

Before each effectful child starts, enroll its permitted creation paths with the existing process owner.
Retain exact process, terminal, stream and artifact handles until their owners finish.
Require the applicable custody result before replacement or graceful removal.
`Exited` means the retained direct process exited. It does not mean the complete effect family ended.
`Quiescent` requires native family exclusion and the independent effect witnesses required by the declared profile.
`Unresolved` retains the resource record, rejects conflicting replacement and identifies the failed custody check.
Do not convert signal delivery, endpoint loss, manager status, lock acquisition or elapsed time into `Quiescent`.
The state owner separately retains outstanding external effects after local custody ends.

| Candidate | Direct reuse | Decisive missing property |
| --- | --- | --- |
| Windows Job | Existing non-breakaway Job owner, retained handles and accounting. Enroll before allowing child execution. | Native-trusted alternate process creators can launch outside Job inheritance. The approved profile must prohibit or explicitly own those paths. |
| Mac packaged root | Existing Guardian plus independently activated LaunchAgent. Preserve explicit resource retirement ordering. | A detached descendant can leave the process group. `AbandonProcessGroup=false` cannot establish universal custody. |
| Linux user unit | Native cgroup membership and manager cleanup, with existing strict-profile admission. | Native-trusted code can use permitted external managers or migration paths. Membership of one cgroup cannot prove absence elsewhere. |

FACT: Microsoft's Job documentation excludes `Win32_Process.Create` children from automatic inheritance.
Source: [Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects), retrieved 2026-11-16.
FACT: systemd's `control-group` mode kills remaining processes in the unit's control group.
Source: [systemd 262 kill contract](https://www.freedesktop.org/software/systemd/man/262/systemd.kill.html), retrieved 2026-11-16.
INFERRED: An unrestricted native-trusted process family cannot inherit a complete-custody claim from any activation primitive above.
Keep B-2 open. Do not remove required tools or silently downgrade their profile to satisfy a proof.

#### Lifecycle and retirement order

| Event | Exact owning action | Permitted result |
| --- | --- | --- |
| Discover | Authenticate a compatible live root. Query its current binding and required terminal handoff. | Attach only under fresh client grants. No management authority follows from discovery. |
| Start | Guard current authority. Compare the request's expected control revision. Persist the desired state through D-10's existing state owner. | Admit work only against that current revision and valid custody, writer and outcome state. |
| Desktop close, Quit or crash | Retire the desktop attachment, windows and desktop-owned helpers. | Separately admitted service work continues under D-9. A vetoed Quit changes no committed lifecycle state. |
| Stop | Serialize with D-9 release admission. Close effect release, commit Stop, acknowledge acceptance, then drain and collect custody evidence. | Return Stopped only with the applicable custody result. Preserve indeterminate remote outcomes. |
| Forced OS stop or root death | Revoke reachable bindings. Classify the event as abrupt loss. | Fresh activation remains non-effectful until custody and outcome recovery admit it. |
| Replace or update | Package owner verifies the candidate. Runtime closes old admission, coordinates terminal handoff and proves old custody exclusion. | State owner admits compatible writer/recovery. Runtime then publishes the new Ready binding. |
| Remove | Authorize Stop. Retain its result and unresolved outcomes. Unregister only after the declared graceful custody check completes. | Forced removal reports abrupt loss. Preserve user data unless separate deletion authority exists. |

Bind each request to D-9's owner-issued ManagementAttemptRef and expected_control_revision before enqueueing.
Recheck current authority at the control compare-and-update. Never replace a stale request's revision with the current revision.
A stale Start cannot overwrite Stop. Repeating an attempt returns its retained result without another state transition.
Failed or uncertain Stop commit leaves effect release closed. Query that same attempt before deciding the durable result.
Preserve the Stop request's cleanup and observation routes while refusing new effect release.
Only a new authorized Start may change the persisted Stop revision. It cannot clear custody, continuity or outcome holds.
The updater retains every live executable stage until runtime and terminal owners release it.
Apply existing compatibility policy during discovery. Do not replace upstream reconnect policy with universal exact-version equality.
Consume #1382's terminal handoff prepare, transport, complete, expiry and failure cleanup contract. Do not create another PTY controller.
Treat local managed, externally managed, SSH remote and WSL services as distinct modes with distinct management grants.
Closing a local tunnel or desktop attachment cannot imply permission to stop an external or remote service.

#### Cold-start continuity and restore

PROPOSED: Fresh activation returns D-10's ContinuityUnproven after all retained owner evidence is lost.
Uptime, a saved PID, a database epoch or an acquired lock cannot independently prove safe continuation.
Only an approved continuity witness can release D-10's hold. A retained enrollment tuple is insufficient.
The state owner decides database lineage. The runtime never infers lineage from process continuity.
Controlled restore first closes admission through the existing state/update owner, then follows its durable restore protocol.
Do not create an adapter-owned restore journal. Keep automatic effects held when restore or power-loss provenance remains unknown.

#### First discriminating native proofs

PROPOSED: Run these cases after the isolated-fixture approval. Retain independent process and effect witnesses.

| Case | Positive observation | Single-fault or adverse control |
| --- | --- | --- |
| Peer identity | Admitted host attaches and permitted Use succeeds. | Same-account wrong-image peer obtains no private client binding. |
| Reciprocal root identity | A validated root response precedes credentials and effectful payloads. | A wrong-image root observes only the public challenge, including after connection replacement. |
| Linux client lifetime | Enrolled client remains live after backend Stop and root crash. | Leave the client inside the backend cgroup and observe its independent process witness die. |
| Linux reconnect | The surviving client obtains a fresh binding from a complete retained enrollment. | Crash after each tuple component. Mix generations or remove one descriptor. No old token, PID or partial tuple recreates authority. |
| Registration | Approved registration reaches non-effectful admission. | Disabled registration causes no detached fallback. |
| Arbitration | Two installations share one root and admit one manager. | Installation-specific arbitration yields two management witnesses. |
| Custody | Direct, ordinary-child and detached-child identities are all accounted for. | Kill root and Guardian together, or move a descendant outside the candidate family. |
| Stop race | Committed Stop survives delayed prior Start and OS wake. | Remove the current control-revision check and observe newly admitted work. |
| Removal | Custody reconciliation precedes unregister and stage release. | Hold a live descendant while injecting early unregister. |
| Restore continuity | Unknown cold-start or restore lineage holds affected effects. | Reuse an old snapshot while an independent sink retains the prior effect. |

This candidate establishes no native test result. Linux source analysis is not Mac or Windows product evidence.
Specification drafting and owner review may proceed. Native fixtures require B-1 approval. Product exposure remains blocked.

#### Alternatives and closure condition

PROPOSED: Keep complete external management as the fallback, under the same required journeys and evidence conditions.
That owner must cover provisioning, authentication, update, Stop, custody, recovery, terminal handoff and removal.
A manual endpoint, a bare launchd daemon or ordinary upstream process replacement does not meet that contract automatically.
If unrestricted native behavior defeats custody, compare a qualified externally managed boundary before changing the selected architecture.
Do not silently substitute a VM, require machine privilege, remove tools or relabel the required platform as unsupported.
Q-1 closes only after the exact authentication, profile, custody and lifecycle mechanisms are independently reviewable.
B-2 closes only with the exact native evidence. Proposed mechanism details do not close B-2.

#### Primary-source receipt

Applicability: OS activation, native peer identity and process-family cleanup.
Context7 discovery used `/websites/developer_apple_servicemanagement`, `/systemd/systemd` and `/dahall/taskscheduler` on 2026-11-16.
The Windows result is a third-party discovery lead. Microsoft documentation remains the authority.

| Official primary | Supported claim and short excerpt |
| --- | --- |
| [SMAppService](https://developer.apple.com/documentation/servicemanagement/smappservice) | macOS 13 supports app-owned agents. Registration is "subject to user approval". |
| [Apple helper migration](https://developer.apple.com/documentation/servicemanagement/updating-helper-executables-from-earlier-versions-of-macos) | Use bundle agent plists and "BundleProgram". |
| [Apple unregister](https://developer.apple.com/documentation/servicemanagement/smappservice/unregister()) | For running agents, "the system terminates it". Unregister cannot be an ordered runtime drain. |
| [Apple XPC requirements](https://developer.apple.com/documentation/foundation/nsxpclistener/setconnectioncodesigningrequirement(_:)) | Enforces a "code-signing requirement on incoming XPC connections". |
| [Windows task schema](https://learn.microsoft.com/en-us/windows/win32/taskschd/task-scheduler-schema) | Idle, battery and execution-limit settings are separate. Default execution limit is `PT72H`. |
| [Windows instance policy](https://learn.microsoft.com/en-us/windows/win32/taskschd/taskschedulerschema-multipleinstancespolicy-settingstype-element) | `IgnoreNew` covers "an existing instance of the task". |
| [Windows Jobs](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects) | Alternate creator children "are not associated with the job". |
| [systemd 262](https://www.freedesktop.org/software/systemd/man/262/systemd.kill.html) | Cleanup covers "the control group of this unit". |
| [Linux cgroup v2](https://www.kernel.org/doc/html/latest/admin-guide/cgroup-v2.html) | "A process can be migrated to another cgroup." |
| [systemd v262 scope source](https://github.com/systemd/systemd/blob/v262/src/core/dbus-scope.c#L143-L171) | `PIDFDs` passes retained descriptors into process attachment. |
| [systemd 262 scope](https://www.freedesktop.org/software/systemd/man/262/systemd.scope.html) | Scope units manage "externally created processes". They do not create the hosts. |
| [systemd v262 descriptor source](https://github.com/systemd/systemd/blob/v262/src/core/service.c#L662-L714) | Generic descriptors pass `fstat`. Non-pollable descriptors are accepted. |
| [systemd 262 service](https://www.freedesktop.org/software/systemd/man/262/systemd.service.html) | Descriptor preservation and capacity require explicit settings. |
| [systemd 262 notify](https://www.freedesktop.org/software/systemd/man/262/sd_notify.html) | Submission only acknowledges enqueueing. Named removal and notification barriers are separate operations. |
| [systemd 262 manager](https://www.freedesktop.org/software/systemd/man/262/org.freedesktop.systemd1.html) | Descriptor-store readback exposes names, inode metadata and flags, not descriptor contents. |

All primary sources above were retrieved on 2026-11-16. Native mechanism sufficiency remains explicitly unqualified.
