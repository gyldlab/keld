# Admission contract candidate

Part of: [index](index.md)
Owns: Proposed service admission, typed interfaces and shared-owner bindings.

## 4. Design: exact admission candidate

### D-9: Exact admission and release contract

PROPOSED: This candidate defines intended API and permission changes for review.
No identifier below is an existing allocation unless labelled FACT.
Implementation requires the exact specification approval and affected owner gates.

#### Decision atoms

| Atom | Owner and boundary | First falsifier |
| --- | --- | --- |
| Peer binding | Runtime, native evidence to private client context | A foreign application obtains a context. |
| Authorization | Guard, context and action to borrowed permit | Connect authority admits Stop. |
| Durable claim | Core state owner, reference and inputs to dispatch token | Two callers obtain releasable tokens. |
| Release | Effect owner, token and authority to executor queue | Revocation wins but release is admitted. |
| Control order | Runtime/state owner, captured request revision to desired state | A delayed Start overwrites a later Stop. |
| Independent work | Runtime/core, explicit service-owned admission to execution context | Desktop detachment prevents the next authorized background effect. |
| Delivery | Bridge, outcome and attachment to reply | An old reply settles a successor call. |
| Capacity | Transport owner, reservations to retained work | Data saturation prevents Stop progress. |
| Codec | IPC and generator, declarations to bytes | Field reorder escapes freshness and wire tests. |

The durable claim neither authenticates a peer nor permits release.
Release does not prove outcome durability. Delivery retirement does not remove outcomes.
Capacity reservation precedes acceptance. Release consumes that reservation.

#### Existing owners

FACT: Generic HELLO proves token possession and consumes one child connection.
Source: `docs/architecture/02-ipc.md:249@413c128e5375df3ffa241809db651bf7fd27e498`.
FACT: dispatch_privileged lends a callback-only ScopePermit after Guard permits a request.
Source: `crates/keld-ipc/src/guard_dispatch.rs:48@413c128e5375df3ffa241809db651bf7fd27e498`.
FACT: Principal has no installed-application identity.
Source: `crates/keld-guard/src/lib.rs:136@413c128e5375df3ffa241809db651bf7fd27e498`.
FACT: The bounded generator already emits filesystem and authentication enum bindings and codecs.
Source: `packages/@keld/kipc/scripts/echo-codegen.ts:157@413c128e5375df3ffa241809db651bf7fd27e498`.
FACT: General keld gen and @keld/schema remain unimplemented.
Source: `docs/architecture/06-runtime-and-tooling.md:503@413c128e5375df3ffa241809db651bf7fd27e498`.
PROPOSED: Extend that bounded generator for service DTOs. Retain the canonical TypeScript-first destination.
Do not add an IDL, permission evaluator, transport owner or outcome journal.

#### Private client binding and permissions

PROPOSED: Runtime creates BoundServiceClient only from Q-1's authenticated client admission.
That admission can retain an owner-spawned private channel or a reciprocally authenticated native peer.
Guard evaluates action grants. Guard does not mint the native peer identity.
The following fields are private. Wire decoding cannot construct the context.

| Field | Exact binding |
| --- | --- |
| application | Verified installation identity and original host-minted principal, including its generation |
| target | Verified declaration, ServiceIdentity, mode and currently admitted artifact |
| service_generation | Q-1 owner's fresh generation |
| attachment | Fresh 16-byte identifier, retired at disconnect or navigation |
| policy | VerifiedManifest, manifest digest, revision u64 and revocation state |

Use AppProcess only with the verified policy for its own application.
Forwarded renderer or plugin calls retain their original principal.
Runtime rejects Unverifiable peer evidence before Connect.
UID, SID, PID, path or bearer possession alone cannot mint this context.
Reconnection replaces the attachment and preserves operation references.

PROPOSED: Guard owns these candidate permission fields and their validation.
They are not current parser guarantees.

```text
app.service.<action> = ["<mode>/<declared-service>"]
mode = managed-local | external-local | ssh | wsl
declared-service = [a-z0-9][a-z0-9._-]{0,63}
action = connect | use | query-outcome | cancel
       | activate | adopt | start | stop | replace | update | recover
```

The package configuration declares each alias once.
Runtime resolves the alias to Q-1's retained target before Guard evaluation.
The request cannot replace the target or mode after that check.
Guard's existing manifest preparation rejects wildcard, duplicate and unknown-mode service scopes.
ScopePermit selects the original manifest grant. A broker cannot select a different grant.
Each action needs its own grant. Use also needs the underlying effect's existing grant.
Query-outcome covers authorized inspection of operation outcomes and management attempts.
A local grant never authorizes external, SSH or WSL management.
Those modes consume their existing approved management owners.
Activate starts only a non-effectful root. Desktop Quit retires only its attachment.

PROPOSED: Runtime and core jointly bind accepted service-owned work to a private AdmittedExecution context.
Runtime owns identity and current policy lookup. Core owns the durable execution and its declared lifetime.
The context retains the original actor, target, policy reference and service generation.
Runtime proposes a fresh service-execution principal for this explicitly admitted lifetime, using its existing principal owner.
Guard approves an immutable scope attenuation from the initiating actor's permitted service and effect grants.
Each release intersects that attenuation with the application's current verified policy. It cannot inherit newly broadened grants automatically.
It is not a cached permission decision or an escaped ScopePermit.
The effect owner evaluates current service and effect grants at each release.
The existing core execution owner supplies background wake, scheduled continuation and recovery callers.
Only those callers can obtain the context from the protected execution binding.
An execution identifier, receipt or client payload cannot construct the context or change its lifetime.

Service-owned acceptance explicitly transfers the admitted work and its reservation to the service owner.
Desktop detachment then retires incoming calls and delivery, without revoking that separately admitted work.
Normal desktop-principal retirement does not retire the accepted service-execution principal.
Foreground work does not acquire this lifetime implicitly. Renderer and plugin requests retain their original principals.
Do not promote an initiating renderer or plugin to AppProcess when evaluating the attenuation.
Execution-principal retirement, policy revocation, Stop and outcome holds still restrict release.
Recovery reconstructs a context only after current policy, runtime custody and Q-3's recovery checks pass.
It never restores a saved grant decision or predecessor release token.

#### Proposed typed interfaces

PROPOSED: IPC owns wire DTOs. Runtime owns authenticated contexts and service control.
The existing effect owner validates arguments. The existing core owns operation references and outcomes.
OperationRef and StoredOutcome have one schema in [D-10](outcomes.md#d-10-exact-outcome-and-storage-contract).
IPC generates their transport representations from that contract. Do not maintain an adapter copy.

```rust
pub enum ServiceCommand {
    Use { operation: OperationRef, descriptor: EffectDescriptorV1 },
    QueryOutcome { operation: OperationRef },
    Cancel { operation: OperationRef },
}
pub enum ServiceReply {
    NotAdmitted { reason: Refusal },
    Pending { outcome: StoredOutcome },
    Outcome { outcome: StoredOutcome },
    Cancellation { result: CancelResult },
}
pub enum Refusal {
    InvalidInput, InputConflict, ExpiredReference, ExecutionHeld,
    Capacity, BindingRetired, UnsupportedMode, Unavailable,
    ControlConflict { current_revision: u64 },
}
pub enum CancelResult {
    Prevented { outcome: StoredOutcome },
    Requested { outcome: StoredOutcome },
    AlreadyTerminal { outcome: StoredOutcome },
}
pub enum ManagementAction {
    Activate,
    Adopt { candidate: AuthenticatedServiceCandidate },
    Start,
    Stop,
    Replace { artifact: VerifiedArtifactSelection },
    Update { package: VerifiedPackageSelection },
    Recover,
}
pub struct ManagementAttemptRef(pub [u8; 16]);
pub struct ManagementRequest {
    pub attempt: ManagementAttemptRef,
    pub expected_control_revision: u64,
}
pub enum ManagementPreparation {
    Prepared { request: ManagementRequest },
    NotAdmitted { reason: Refusal },
}
pub enum ManagementCommand {
    Apply { request: ManagementRequest },
    Query { attempt: ManagementAttemptRef },
}
pub enum RegistrationState {
    Enabled, Disabled, RequiresApproval, Absent, Conflicting, Unverifiable,
}
pub enum ManagementReply {
    NotAdmitted { attempt: ManagementAttemptRef, reason: Refusal },
    RegistrationRefused { attempt: ManagementAttemptRef, state: RegistrationState },
    Pending { attempt: ManagementAttemptRef },
    Observed { attempt: ManagementAttemptRef, control_revision: u64, state: ServiceState },
}
pub enum ServiceState { RootReady, Running, Stopped, CustodyUnresolved }

pub fn service_call(client: &BoundServiceClient, command: ServiceCommand,
    capacity: CallReservation) -> Result<ServiceReply, CallError>;
pub fn admit_execution(client: &BoundServiceClient, execution: &PreparedExecution,
    capacity: ExecutionReservation) -> Result<AdmittedExecution, CallError>;
pub fn prepare_management(caller: &AuthenticatedAppHost, target: &RetainedServiceTarget,
    action: ManagementAction, expected_control_revision: u64, capacity: ControlReservation)
    -> Result<ManagementPreparation, CallError>;
pub fn control_service(caller: &AuthenticatedAppHost, target: &RetainedServiceTarget,
    command: ManagementCommand, capacity: ControlReservation)
    -> Result<ManagementReply, CallError>;
pub fn release_effect(execution: &AdmittedExecution, claim: &CommittedDispatchClaim,
    effect: BoundEffect, capacity: EffectReservation)
    -> Result<ReleaseReceipt, CallError>;
```

These signatures are proposed, not present at the source pin.
Control revisions admit zero through 9,223,372,036,854,775,807. Refuse exhaustion before incrementing.
PreparedExecution is core-owned and private. It carries the original access binding and explicit lifetime admission.
Only existing owner-private resolvers create candidate, artifact, package and retained-target values.
AuthenticatedAppHost preserves the initiating principal and its verified application policy, including forwarded calls.
Activate and Adopt use the local control entry before a service endpoint exists.
Replace and Update consume package-owner selections. Callers cannot submit executable paths.
Guard denies return the existing CallError. They do not expose target existence.
NotAdmitted requires proof of refusal before logical acceptance.
Unknown completion after acceptance returns Pending or an indeterminate Outcome.

The entire outcome envelope fits Q-3's 16 KiB limit.
Terminal states require Q-3's owner-verified outcome and matching receipt. A caller cannot assert either value.
Pending carries Prepared or DispatchIntent. Outcome carries any other state.
Prevented requires CancelledNoEffect and an authoritative NoRelease receipt.
Requested preserves the current state while interruption remains pending.
AlreadyTerminal preserves the exact committed terminal outcome.
QueryOutcome requires current authority even when the caller knows the reference.
Core retains the immutable actor, mode, target and operation access binding alongside the persistent reference.
The query and cancellation owners resolve that binding before Guard evaluates the current action.
The caller cannot substitute another actor or target. Cross-actor access requires an explicitly approved delegation.
QueryOutcome calls the read-only Q-3 operation. It needs neither writer admission nor the original input digest.
Stop, execution holds and uncertain continuity do not prevent otherwise authorized inspection.
Cancel uses Q-3's cancellation operation. The state owner supplies its own writer admission for any transition.

#### Management attempts and control ordering

PROPOSED: The existing runtime/package owner issues ManagementAttemptRef before any state-changing control admission.
Its existing retained attempt record binds actor, target, action, selected inputs and the captured expected control revision.
Use the core desired-state record or package transaction owned by that action. Do not add an adapter journal.
Prepare validates current authority and reserves bounded attempt storage. It cannot release effects or change desired state.
Lost preparation replies may leave unused references. They cannot create accepted management work.

Apply resolves the issued reference and compares every immutable request field with its owner record.
Unknown, retired or fabricated references refuse. Changed inputs return InputConflict.
Authorize before disclosing whether a reference exists. Foreign and unknown references cannot reveal another actor's attempt state.
Apply then rechecks the original action's current grant and compares the captured control revision atomically with admission.
An obsolete revision returns ControlConflict. Never substitute the current revision into an old request.
A fresh authorized request must obtain a new owner-issued attempt.
An unchanged retry of an accepted attempt returns that attempt's retained disposition or Pending.
It does not repeat admission after another attempt changes the service state.

Query resolves the same attempt under current query-outcome authority and the immutable access binding.
It does not acquire writer admission or release work. Observed reports that attempt's recorded state and revision.
Pending preserves uncertainty about admission or completion. It cannot assert non-admission.
Only a proven pre-admission refusal returns NotAdmitted.
RegistrationRefused carries the observed non-Enabled state after authorization. Enabled cannot appear in that refusal variant.
The caller matches the typed state, not an error message.

Serialize Stop with the effect owner's release-admission boundary.
Close release admission before committing Stop's desired-state comparison. Acknowledge accepted Stop only after that commit.
A failed or uncertain commit leaves release closed until the same-attempt observation resolves its disposition.
Do not reopen admission based on timeout, connection loss or a new identifier.
Previously admitted effects retain their possible-effect outcomes. Stop preserves its cleanup and observation routes.
Fresh Start cannot clear custody, continuity or outcome holds.

Retain accepted attempts through unresolved completion. Retired references cannot become new attempts.
Bound prepared and retained attempts through the existing owner's quota and retirement contract before implementation.
Reserve completion capacity before accepting control. Exhaustion refuses new preparation while preserving existing observation and cleanup.

#### Descriptor, identity and argument handoff

PROPOSED: Core prepareNew creates the reference, mapping, Prepared record and reserved capacity in one transaction.
The mapping binds session, execution, message, call and effect ordinal as applicable.
Retries reuse the issued reference. Unknown or expired references never become new work.
Q-3 owns issuance, rejection floors and deliberate new-attempt admission.

EffectDescriptorV1 has these ordered fields:
1. Existing effect-owner key and that owner's operation key.
2. Typed argument schema digest and argument bytes, together at most 24 KiB.
3. Environment snapshot digest, ordered secret-version references and plugin revision digest.
4. Q-3's stable upstream binding.

The approved dispatcher owns the closed owner-key mapping. Unknown owner keys refuse before a handler runs.
That owner decodes and validates its existing argument schema.
The owner retains resolved resources and returns BoundEffect with canonical argument bytes.
No opaque argument blob reaches an OS call or generic privileged proxy.
Owner dispatch consumes the retained BoundEffect, not another parse of the caller's bytes.

Hash the canonical descriptor with the bound stable actor, service mode and retained target identity.
For hashing, target identity means D-6's ServiceIdentity: installed component, account and canonical physical data root.
Exclude the admitted artifact and owner generation from that stable identity. Check current artifact compatibility separately before release.
Use BLAKE3 over the prefix KELD_SERVICE_EFFECT_V1 followed by a zero byte and the canonical postcard tuple.
Tuple order is schema version 1, stable actor, mode, retained target identity, then the ordered descriptor fields.
FACT: BLAKE3 already has a workspace pin.
Source: `Cargo.toml:68@413c128e5375df3ffa241809db651bf7fd27e498`.
Sort maps by encoded key and reject duplicate keys.
Secret values remain with the credential owner. Bind only opaque secret versions.
Exclude volatile attachment, service and writer generations from the stable retry key.
Release checks current execution authority, service generation and writer admission.
Delivery separately checks the receiving attachment and its current authority.
Changed inputs return InputConflict. Matching retries return the existing record.
Recovery rebinds saved typed inputs under current authority and compares the original digest before possible release.

#### Release and revocation order

PROPOSED: Q-3 atomically claims Prepared using version, digest and current writer admission.
The transaction persists DispatchIntent and its 16-byte token before external release.
Only that winning claim reaches release_effect. No external work runs inside the transaction.
The effect owner serializes current-authority checks and token consumption with revocation.
It also checks current desired state and the execution's continuation holds before release.
Serialize that check and queue insertion with the execution closure protocol below.
An acknowledged closure before queue insertion prevents release. Persisting a database flag alone cannot establish that closure.
The claim, BoundEffect and reserved capacity must belong to the same operation and immutable input digest.
Successful insertion into its reserved executor queue is the release-admission point.
Transport queuing, adapter Ack and Prepared are not that point.
The effect owner retains BoundEffect until execution completes or authoritative refusal releases it.
Do not hold Guard or storage locks during external I/O.
Do not return a reusable release permit to a caller.

Revocation first refuses release and retains no-release evidence.
Release first means effects remain possible until the outcome owner establishes more.
A crash that loses ordering evidence preserves Indeterminate.
A new generation cannot reuse a predecessor token.
Q-3 conditionally stores a late verified receipt without overwriting committed success.
Retired delivery drops the reply, not the committed outcome.

#### One-use release records

PROPOSED: The actual effect owner keeps one shared record for the operation, token and effect-owner generation.
Reserve that record and its capacity before the core can return a releasable claim.
Reservation grants no release authority. Bind its opaque reference to the proposed token and immutable input digest.
A failed claim retires the reservation. An ambiguous commit preserves the original reservation and pending outcome.
The core sends bounded authenticated commit references across IPC. It does not transmit a Rust capability value.
The effect owner validates the current core writer, committed operation, token, reservation and immutable inputs.
Only that owner can construct the local CommittedDispatchClaim witness. Wire decoding cannot construct it.

Under the release gate, Reserved changes once to Consumed or Retired.
Only Reserved-to-Consumed inserts work into the reserved executor queue.
Repeated reserve or consume returns the retained disposition without another insertion.
Current QueryOutcome authority still governs any retained outcome disclosure.
Unknown or retired reservation references refuse. Saved intent cannot reconstruct a missing release reservation.
Retain consumed and retired records until generation retirement and delayed-message fencing make their references unusable.
Owner loss leaves durable intent uncertain. Missing memory does not prove non-release.

Bound release records to 1024 per service, including consumed and retired records.
Bound their complete storage, indices and retained dispositions to 1 MiB, charged against the existing 8 MiB buffer cap.
At either limit, refuse new reservations and preserve independent Stop, revocation, inspection and cleanup.
Do not evict consumed records to regain capacity. Generation turnover requires the ordinary custody and retirement checks.
These are experimental bounds. Full journeys must qualify them before shipping.

#### Execution closure and owner enrollment

PROPOSED: Runtime owns the registry of every effect owner capable of releasing the affected execution or causal dependent.
Registration and closure share one ordered registry revision. D-10 owns persisted causal dependencies and hold intent.
Begin closure before persisting a new uncertainty or conflict hold.
Each owner closes its release gate and invalidates unconsumed affected reservations before acknowledging its generation and closure revision.
Complete closure requires every capable owner's acknowledgement or approved effective fencing.
Missing acknowledgement leaves Pending/Unresolved. Persisting conflict evidence does not certify that unreachable release is excluded.
Keep reached gates closed. Preserve possible effects from earlier consumes and unresolved owners.
If hold persistence fails, keep admission closed and retain the unresolved closure through the existing continuity owner.

A newly joined or restarted owner remains non-effectful until it installs current closures and causal dependencies.
Bind that installation to the current registry, dependency and closure revisions before enabling release.
An owner-set or dependency change invalidates stale closure-completion and reopen witnesses.
Reopen requires current authority, durable reconciliation and those exact current revisions.
No gate lock spans storage synchronization or external work. A database write cannot replace the owner acknowledgements.

Limit the live registry to 64 capable owners, 1024 held executions and 4096 causal edges per service.
Charge their complete live storage to a separate 1 MiB allowance within the existing 8 MiB buffer cap.
Refuse excess enrollment or dependency admission before release becomes possible. Never discard a live hold to regain capacity.
Reserve closure-state capacity before admitting an execution. Unrelated allowed work and independent control progress remain available within capacity.

#### Codec and resource binding

PROPOSED: Extend IPC's closed enums and bounded generator for these DTOs.
Use keld_ipc::codec and existing TypeScript primitive codecs.
Golden vectors bind field order, enum order, integer domains and rejection behavior.
Reject excess lengths, unknown variants, trailing bytes and noncanonical identifiers.
Q-1 and IPC own the service endpoint protocol and its authenticated channel binding.
Ordinary child HELLO remains unchanged.
The shared channel owner must approve its concrete allocation and receiver policy before implementation.

PROPOSED: Runtime owns one budget structure. All values below are experimental bounds.
D-6 retains 8 pending calls/client, 64/service and its request, response and stream byte limits.
Add 8 authenticated clients/service and 32/root. Allow 4 combined pre-authentication and rebind attempts/root.
Each handshake envelope is at most 4 KiB.
Each handshake has a 5-second absolute deadline.
Add 8 long-lived leases/client and 64/service, including terminal and stream leases.
Each live stream uses the existing credit owner and charges D-6's aggregate stream budget.
The 8 MiB retained-buffer cap includes partial frames, decoded values, replies and intermediate copies.
Reserve capacity before acceptance. Release every reservation once.
Service-owned execution transfers its lease charge before releasing the originating RPC reservation.
Detachment does not release that execution's service or original-client charge. Reconnection cannot reset those charges.
A ready Stop/revoke event takes the next admission turn through D-6's independent permit.
Bound each handler step. A blocked physical write keeps its existing absolute I/O deadline.
Expire an unresponsive route truthfully and retain unresolved outcomes.

The existing virtual-port and terminal owners keep their wire, replay and loss contracts.
This service-control DTO is not a privileged byte tunnel.
Native engine/kernel allocations require their own measured, platform-specific envelope.
U-1 and native qualification own that measurement. They do not prevent writing or approving this candidate.
If full journeys exceed these limits, reopen the budget or transport before exposure.
Do not silently truncate output, remove required functionality or claim unmeasured performance.

#### Remaining approval bindings

Q-2 remains open for peer authentication, exact generated codecs and shared-owner allocation review.
The new execution-principal minting, lifetime and Guard attenuation binding also require exact owner approval before implementation.
The current Principal enum does not provide this service-execution principal. D-9 proposes that extension without claiming existing support.
Management-attempt storage, crash recovery, quotas and retirement need exact runtime/package/core bindings before Q-1/Q-3 can close.
The authentication dependency is a design binding. Native behavior and capacity remain later qualification gates.
