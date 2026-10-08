# Spec: reliable full-package activation before delta optimization

Status: approved
Linear: KEL-53 · Owner: GYLDLAB · Updated: 2026-09-26
Approval: original corrected contract: Linear comment `b343d835-1528-461f-bda4-0fa5e238b5af` · approved corrected content head `a340acf0b5cfcbfab9111f938cd3ac2788219ccb` · decision SHA-256 `972b82947189b5d89c7c78d11547f0c0ef890a60bf36af1bdfb68d448fed2ed9`; KEL-263 producer-host and policy-owner amendment: delegated approval comment `df61a6f6-3215-44a8-8780-7ae242fc74ab` · decision SHA-256 `871a803ba4c04087209ebb7a19751a15382dcae42cd8764d2cd7090d3cc9ba83` · approved content head `3d5d51e31237c365847a1e9b866880ba06429e2e`; complete crate dependency graph: supplemental delegated decision comment `212a2039-f729-4708-a983-9a348d299bd5` · decision SHA-256 `939cc2100ba25b172088371c1311e7555156c0f8e9430dd8ad38998caa5d0fc8` · approved content head `3d5d51e31237c365847a1e9b866880ba06429e2e`.

{"schema":"keld.kel53-approval/v1","decision":"approved","approved_content_head":"a340acf0b5cfcbfab9111f938cd3ac2788219ccb","approver_id":"49ccfebb-c3fb-40a3-abb2-a3bf92e83cb1","linear_comment_id":"b343d835-1528-461f-bda4-0fa5e238b5af","source":"active-maintainer-session-2026-09-20"}
{"schema":"keld.kel53-approval/v1","decision":"approved","approved_content_head":"3d5d51e31237c365847a1e9b866880ba06429e2e","approver_id":"49ccfebb-c3fb-40a3-abb2-a3bf92e83cb1","linear_comment_id":"df61a6f6-3215-44a8-8780-7ae242fc74ab","source":"delegated-maintainer-session-2026-09-26"}
{"schema":"keld.kel53-approval/v1","decision":"approved","approved_content_head":"3d5d51e31237c365847a1e9b866880ba06429e2e","approver_id":"49ccfebb-c3fb-40a3-abb2-a3bf92e83cb1","linear_comment_id":"212a2039-f729-4708-a983-9a348d299bd5","source":"delegated-maintainer-session-2026-09-26"}

Windows multi-mode product decision and T1 synchronization authorization: owner decision
recorded in Linear comment `e5572b4a-9643-4847-8ed2-c419e2198fc6`, bound to the
pre-receipt activation-spec SHA-256 `4213CE3E9BED2EA380622A162763702EC321DC50297E2DB275844A983B73EAFF`
and Architecture 06 SHA-256 `F14586C97AA475561A15DDFDB988E839E3FB6594DE3407C65669AA5EF3F8D40E`.
The no-repeat-UAC product mode is approved; its specific privileged mechanism is not.

KEL-265 T3b amendment: delegated approval comment
`559df4c6-b8b1-4a47-ae81-2311e2743eb6`, approved content head
`bb3d863e20d495349aa64807bafa598ffa4f31b0`, file SHA-256
`f9ab8891c061150b8f3fe90884b181fbd6ebd11ff49b605b8c6570a5904f800c`.
The maintainer delegated this bounded decision in the active session; native
acceptance and independent implementation review remain required.

KEL-266 T4a amendment: delegated approval comment
`a059df2c-e9fe-4eec-9582-92b6aa5cedb0`, approved content head
`a5fc30808e61f8c1c0707759c40190c4efbec7f8`, file SHA-256
`649b2f97d72a06ebd61a2fca23721a1ba75e28f9477a7252c6961ce5071e0ba5`.
The independent design approval does not replace native qualification or final-diff
security, unsafe and public-contract review.

KEL-270 T4b amendment: owner approval comment
`a0329498-a115-4bcb-a277-0f32a42d7a86` (all four points), approved content head
`caf2c82b52b2c4a95d5fd0ce2003e611267439e2`, file SHA-256
`4355824b8514acd51713ff920b5b4143759a9f3563209eab3df4a51707ddefed`. The owner approved it
in the active maintainer session on 2026-10-02. Four bounded changes to the common
transaction, each narrowing a liveness or replay gap; none grants authority beyond the
existing exclusive writer lease, and item 4 adds one public repair entry point:
1. Retire each unreferenced version under the journal before journal removal, and
   remove the journal by write-through rename, instead of removing the journal first
   and cleaning retention afterwards.
2. Resume an unlaunched `PublishPending` attempt under the exclusive writer lease alone.
3. Durably re-mint the health and lifecycle channel identities whenever an attempt is
   resumed.
4. Add an explicit repair entry that, under the exclusive writer lease and only when no
   journal exists and every record validates, retires complete versions that no record
   references; the ordinary loader still halts on them.
Rationale and native evidence are in KEL-270 comments `b571afdb` and `f6b1e538` and the
T4b pull request.

KEL-270 F1 amendment: owner approval comment
`9c84d37c-7f13-43ef-b5ec-fb5bc189bb81` (2026-10-05), recorded with the approved proposal
in execution artifact `adaa572e-9b3a-4528-8f6d-1b644782470e`, after review of the
pre-journal orphan window that T4b item 4 could only repair. It removes that window at its cause instead of repairing its result:
1. A completed stage keeps its generated `incomplete-*` name. The `PublishPending`
   journal is written first, and renaming the stage to its version name is the first
   journaled step. A crash before the journal is durable leaves only a stage, which the
   census already tolerates and the next resolution removes.
2. The trust floor advances only after the published candidate is fully re-verified and
   pinned. A published copy whose own content fails verification is retired under the
   journal and the next stage recording the exact candidate is tried; with none left the
   attempt is abandoned with no record changed. A census fault about other entries, or
   a fault reading a stage, keeps the journal for journal-bound recovery.
3. A `PublishPending` journal whose candidate is neither published nor staged, with the
   floor still at the recorded prior floor, is abandoned: the journal is removed and no
   record changes. The journal schema is unchanged; recovery finds the stage by its
   completion record, which must name the exact journaled candidate.
4. `ExtractedWindowsStage::publish_version` is replaced by `complete`, and
   `WindowsExtractionRoot::begin_activation` takes the resulting `CompletedWindowsStage`.
   The explicit unjournaled-version repair stays for installations that already hold an
   orphan from the earlier order.

KEL-254 A3 cross-reference (2026-10-05): the §4 lifecycle installation-ID sentence and
the trust-anchor sentence name the KEL-254 executable-located anchor, and KEL-254 T2b
adds a KEL-53 loader entrypoint; exact content approved with KEL-254 A3 by
Linear comment `859b62fb-431c-44c1-8346-5621e65e04ec` (PR #374 head `b284d39ab2a479898b4bb53a6ae7d36e80ee3037`).

KEL-270 T4d amendment (Machine-UAC activation decisions): owner decisions recorded in the
active maintainer session on 2026-10-05. The amendment was split out of PR #374 on
2026-10-05; the approval that #374 received covers only KEL-254 A3 and T2b/T3, not this
amendment. Its exact content was approved by owner approval comment
`1cdcf977-7f30-4cf9-ad6d-50e03cf73880` (2026-10-06), binding PR #384 head
`6944917ddf35e405bb1956d3e4886c488534b814` and the approved spec-content SHA-256
`659a40449d06c10aba04e6c3e8f3f7e7dc2277e7547fe65389cc3c9b153311f3`. The review
resolutions made after the split (claimant binding, endpoint squatting, the typed
`MachineRecoveryRequired` effect, the helper crate and its FFI owners, and the review
batches of rounds 1 to 3) are part of that approved content. The SHA-256 above binds the
PR #384 head only: a later reviewed PR may amend this file, and every such amendment
cites its authority (an owner decision, a Linear coordination record or the review that
required it) in the amended text itself. It optimizes for least privilege and the
smallest privileged surface:
1. Candidate launch uses the exact initiating-process token, which the helper takes from
   the verified host process object ("Machine-UAC bootstrap").
2. A dedicated minimal signed `keld-updater-helper.exe` is the elevated component; the
   application host is never elevated (criterion 17).
3. Reboot or loss of every owner during any attempt fails closed into the typed
   `MachineRecoveryRequired` state with a supported helper recovery-only role that needs
   no ordinary host boot ("Machine-UAC recovery-required state and recovery-only role").
   For a launched attempt its retirement evidence is the existing writer-lease rule plus
   termination of the initiating user's logon session; the kill-on-close attempt Job
   only ends the candidate promptly ("Machine-UAC owner-loss retirement"; the owner
   delegated this mechanism in the PR #374 review). Until the T4d rows pass, the
   recovery role is disabled and administrator action is the only resolution.
4. Candidate health in every direct mode uses an authenticated one-shot connect-back
   endpoint, replacing the inherited candidate endpoint; the candidate receives only a
   rendezvous name that carries no authority ("Candidate connect-back").
5. Any keeper is minimal and attempt-scoped; the helper itself prefers to own the attempt.
6. The privilege-crossing bootstrap and health path uses its own versioned subprotocol,
   reusing only low-level framing, nonce, deadline and peer-verification utilities.

Owner decisions of the round-1 review (2026-10-05), each applied in its owning
paragraph:
- **D1, unlaunched `publish-pending` under owner loss (refined 2026-10-06, "abandon
  intent"):** the recovery-only role never launches and resolves such an attempt with an
  abandon intent inside the one `next_activation_step` state machine, so every
  intermediate state stays `PublishPending` ("Machine-UAC recovery-required state and
  recovery-only role"). It needs five abandon-intent-scoped step-mapping changes and a
  `retirement_due` change, all listed in §5, and leaves the ordinary intent unchanged.
  Rationale: the landed step order advances the floor before
  `AwaitingHealth` (`AdvanceFloor`, `SelectCandidate`, `EnterAwaitingHealth`:
  `windows_baseline/activate.rs:745-765`, `activation.rs:280-297`), so the first D1
  route through `resume_unlaunched` and `roll_back` always consumed the signed version,
  even when the floor had not moved, and left a crash window from `AwaitingHealth`
  through `RollbackPending` until journal removal that looks launched. Rejected: letting
  the recovery role launch the candidate; that first route through `resume_unlaunched`
  and `roll_back`; a journal marker for "owned by a role that never launches", which
  still consumes the version because `RollbackPending` requires the floor at the
  candidate (`activation.rs:340-342`) and adds a v2 variant and a new invariant; and
  accepting the residual window.
- **D2, bootstrap shape:** the host is the pipe server, admitting only its own user SID
  and BUILTIN Administrators, and verifies the connecting helper by exact process-object
  identity against the process handle that `ShellExecuteExW` returned; the elevated
  helper is the client, verifies the host process, takes the initiating token from that
  same process object, pins the staged source by token-based thread impersonation in
  `keld-guard` rather than pipe impersonation, and receives exactly one rendezvous
  argument that carries no authority ("Machine-UAC bootstrap"). Rejected: anchoring on
  the helper's parent process, which relies on undocumented UAC re-parenting and a new
  parent-process-ID FFI; the WTS session user, a name-based binding; and carving the
  bootstrap out of this approval.
- **D3, no claimant journal read:** the claimant never reads the journal before
  authentication; the owner passes it a rendezvous name at launch, and any other launch
  while a writer holds the lease refuses with a typed `WriterActive` without reading
  (criterion 20, "Candidate connect-back"). This also removes a race: a file that has
  open handles, or whose replacement target has open handles, cannot be renamed
  ([FILE_RENAME_INFORMATION](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_file_rename_information),
  `ms.date` 2021-11-22), so a pre-authentication reader could fail the writer's
  `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`. Rejected: a POSIX-semantics rename through
  `SetFileInformationByHandle`, which would change the landed durable primitive in
  every mode and need an `unsafe` amendment; and documenting the blocked-replace
  residual.
- **D4, KEL-135 Windows Authenticode verifier home:** the verifier moves from
  `keld-core` into `keld-guard`, which already owns the token FFI and the Windows ACL
  validators; `keld-core` and the helper both call that single owner, and copying it is
  forbidden (§5). Rejected: a new crate.
- **D5, Machine-UAC claimant fallback:** every connect-back endpoint that the elevated
  helper creates carries an explicit `O:BA` owner (the host-created bootstrap endpoint
  keeps the host's own user SID as owner), and when a Medium claimant cannot open the
  elevated owner it
  checks that owner together with the server's process ID and session; a Medium
  squatter cannot make BUILTIN Administrators the owner ("Candidate connect-back").
  Rejected: adding a query ACE to the helper's own process.

Each new production `unsafe` path still requires its owner's AGENTS.md update and
independent unsafe, privilege/security and wire review before implementation; §5 names
the new `crates/keld-updater-helper` crate and the owner of each call, §6 T4d orders the
slices, and §8 records the gates.

KEL-270 T4d wire-layout amendment: owner approval comment
`eff8e2fb-9efd-46dd-bd8c-390d66ea04f2` (Linear KEL-270, 2026-10-06), after three rounds
of independent wire review, takes the recommended option of each of its four decisions
(the single `keld-ipc` locator owner, strict field lists, the §4 receipt digest in `AB1`
and the candidate-mode generation-exit rule) and binds approved content head
`df65f5c5a7dd3ee059798ed1d1f404b400aa82a7`, spec SHA-256
`33547073694aab5494d71226e273f895837a80a550a549c8c2454088a4bd8d64` and Architecture 06
SHA-256 `89ebc48b71fd6bf6767123ebb94e782140708cbd23f6132c2cbd077043bdc2ed`. The commit
that records this approval only replaces the pending tags with the approval citation
(approved: KEL-270 owner decision `eff8e2fb`, 2026-10-06) and the wording that described
the draft as proposed; it changes no rule, byte layout or other citation. The amendment
fixes the byte layouts that approved text left to the T4d wire review before code.
"Candidate connect-back" gains the locator function with its single owner, `keld-ipc`
(*Locator*), and the `keld-attempt` record table with its transcript and health-sequence
rules (*Messages*). Every value that earlier approved text left open, and every rule
that this amendment adds, is listed once, with its rationale, under *Proposals* there.
The same approval binds the rewording of criterion 8 and of Architecture 06 §4a "Health
identity", which then say that the receipt binds the attempt id and the full artifact
identity through the §4 receipt digest, and binds that Architecture 06 paragraph's
statement of the candidate-mode generation-exit rule of the *Health sequence* paragraph.
§6 assigns the purpose-`1` locator and the `is_attempt_endpoint` predicate to S4 and the
bootstrap records and the purpose-`2` locator to S11, §7 adds the codec and
health-sequence rows, and §8 records the new `keld-ipc` edge to the workspace-pinned
`blake3` and the health-receipt digest that the candidate computes. The journal-v2
golden vectors are not part of it; the S3 pull request fixes them under that slice's
wire review.

KEL-266 AC4–6 completion: delegated approval comment
`bfeb14d0-e906-476f-970a-7fd837bc7f2f`, approved content head
`a7d54066704f08cb170435ad72877afdea93f6d1`, file SHA-256
`d43431e186fa20f1ca3d2281dfab06ef9fb8227be5044face730c3390b6896fb`.
This supplement closes coherent admission, higher-release staging and committed-state
LPAC evidence within the same issue; it does not claim activation or installed boot.

Windows direct-install mode amendment: exact content was approved in the user's
approval of Linear PR #290 head `b78c89b061049647421dda69034c9f35079d9441`, via Linear
comment `e0b276f9-5ecc-42a9-ac8f-8a5e05f44245`. The approved pre-receipt KEL-53 file
SHA-256 is `6b4fda6e8bcbb388886d8254d5bc8558cd706533e6c28a061c478dc1acc42442`. This
amendment supersedes any earlier implication that the machine-protected baseline is the
only direct-install mode or that one helper mechanism is selected. Its implementation
and native acceptance gates remain open; in particular, this approval does not select
or authorize the machine-seamless authority.
Post-approval consistency correction: the Machine-UAC summary now matches the detailed
adapter contract below: the request carries only a bounded, non-authoritative source
locator, and the elevated owner opens, pins and verifies the source handles itself.
This resolves contradictory summary wording without changing an install mode or
authority boundary.

## 1. Goal & non-goals

Keld's direct updater must prove one safe signed full-package
install/activation/recovery path. The first admitted package cell is Windows x64
direct distribution whose complete file tree fits the existing v0 canonical archive.
Its default installation mode is per-user with seamless no-UAC updates. An explicitly
selected Program Files installation supports both per-update explicit-UAC activation
and opt-in seamless machine-wide activation, with the latter's privileged mechanism
gated on separate proof. Package/deployment-managed installs defer mutation to their
owner. One updater verifies a signed release, stages exact bytes, journals and publishes
one attempt-bound candidate, confirms its exact health receipt, then commits or recovers
to the retained last-known-good package. Delta patches remain a later optional
transport optimization.

Non-goals:

- no delta algorithm or dependency in the first slice;
- no updater for package-manager/store-owned installs;
- no macOS/Linux package claim before KEL-137 supplies the executable-mode/link
  representation those cells require;
- no data-migrating release, migration hook, or claim that binary rollback rolls data
  back in the first slice;
- no arbitrary relaunch helper, shell command, self-update plugin, or role-writable
  update state;
- no inference of install mode from Program Files, LocalAppData, executable path,
  registry location, environment or writable configuration;
- no selected no-UAC machine coordinator until its named authentication, replay,
  writer, lifecycle, health and recovery proofs pass;
- no TUF-style rotating-root design beyond the existing v0 single-key limitation;
- no implementation before this approved corrected specification lands.

### Windows direct-install modes (approved contract; implementation gates remain)

One KEL-53 transaction owns signed package verification, anti-downgrade policy,
staging, the activation journal, installation-wide exclusive ownership, exact candidate
launch, health confirmation, commit/rollback and crash recovery. Install mode changes
only how that transaction obtains its activation write lease; it MUST NOT create three
updater state machines or move journal/pointer policy into the authority adapter.

| Mode | Install and update authority | Required boundary |
|---|---|---|
| `PerUserDirect` (default) | Install beneath the installing user's application location. Verify, stage and activate under that user's ordinary authority with no UAC. | The same-user owner can mutate its own files; Keld does not claim protection from arbitrary native malware under that account. Supervised app roles remain unable to mutate updater state where the admitted profile supports distinct OS principals. |
| `MachineUacDirect` | Install beneath a protected machine root. Download and verify into a separate user-owned staging area; when protected activation is required, request UAC and use the signed/elevated updater for the exact attempt. | The application host and Bun roles remain non-elevated/non-SYSTEM. The request is bound to the initiating user's SID, logon-session identity and host process, carries only KEL-53's bounded source lookup locator, which conveys no authority, and is handled by the elevated `keld-updater-helper.exe` under "Machine-UAC bootstrap" below, including over-the-shoulder consent. The helper mutates only this installation's package/update roots through common KEL-53 code and accepts no mutation authority from argv, environment, cwd or caller paths. Forged, stale, replayed, wrong-host, cross-install, replaced-source or wrong-session requests refuse before protected mutation. Owner loss is governed by §4 "Machine-UAC recovery-required state and recovery-only role". |
| `MachineSeamlessDirect` | Install beneath a protected machine root; later activation obtains a narrow privileged write lease without repeated UAC. | Opt-in only. The mechanism is not selected by this requirement. It remains unavailable until the KEL-270 proof gate below passes and an exact-reviewed Windows-native authority is selected. Host/Bun remain ordinary-user processes. |
| `ManagedOwner` | MSIX, App Installer, Store, enterprise deployment and package-manager owners perform their own update. | Keld MUST refuse direct activation and MUST NOT register a competing writer or selector. |

**Machine-UAC bootstrap (owner decision D2; a qualification target, not yet proven).**
This paragraph is the single owner of the hand-off from the ordinary host to the
elevated helper. It replaces the earlier `WindowsUacAuthorityAdapter` sketch, under
which the ordinary updater would have transferred a token: a Medium process cannot
duplicate a handle into an elevated process, so the token comes from the helper's own
open of the verified host process instead.

1. *The host is the pipe server.* Before elevation, the admitted host mints a fresh
   bootstrap nonce, derives the rendezvous name from it with the `keld-attempt` locator
   function ("Candidate connect-back") and creates that endpoint as the only instance,
   rejecting remote clients, with a non-inheritable handle, an explicit Medium
   no-write-up label and a protected DACL that grants the landed `keld-ipc` mask to
   exactly two SIDs, the host's own user SID and BUILTIN Administrators; it reads the
   descriptor back. The descriptor's owner is the host's own user SID: a Medium process
   cannot make BUILTIN Administrators the owner, so the `O:BA` owner of owner decision D5
   applies only to the endpoints the elevated helper creates. The host starts the helper
   under "Helper launch and self-anchor" with `ShellExecuteExW`, the `runas` verb and
   `SEE_MASK_NOCLOSEPROCESS`, passing the rendezvous name as the helper's single
   argument; the name carries no authority. `hProcess` is not returned in every case
   ([SHELLEXECUTEINFOW](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ns-shellapi-shellexecuteinfow),
   `ms.date` 2018-12-05), so the host refuses without it; whether an elevated `runas`
   launch returns it, and with which access rights, is a qualification target.
2. *The host verifies the client by process identity and impersonates no one.*
   Immediately after a client connects, under a per-connection deadline, the host
   requires `GetNamedPipeClientProcessId` to equal the process ID of the retained
   `hProcess`, and that handle to be still unsignaled after the comparison. A process ID
   identifies exactly one process until that process terminates
   ([Process Handles and Identifiers](https://learn.microsoft.com/en-us/windows/win32/procthread/process-handles-and-identifiers),
   `ms.date` 2025-07-14), so this is exact process-object identity; when the host can
   also open that process, `CompareObjectHandles` must agree. A mismatch disconnects the
   client and re-arms the same instance (`disconnect_for_retry`) until the one-shot is
   consumed or the bootstrap deadline passes.
3. *The helper is the client and verifies the server first.* Before it opens anything,
   the helper accepts its single argument only in the exact local shape
   `\\.\pipe\keld-attempt-<64 lowercase hex>`, through an `is_attempt_endpoint`
   predicate beside the landed exact-shape predicates in `keld-ipc`
   (`bootstrap.rs:2147-2170`); the only other value it accepts is the exact fixed
   recovery-role selector of "Helper launch and self-anchor", which starts the recovery
   role and no bootstrap. A UNC or remote path, another namespace, uppercase hex, a
   wrong length or any extra argument refuses before any open or write. It opens the
   endpoint with `SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION`, so the host can at
   most identify the elevated helper and never impersonate it. Before it sends
   anything, it opens and pins the process that `GetNamedPipeServerProcessId` names and
   requires its image to be the `keld-host.exe` of the helper's own located
   installation's selected version tree, by file identity, its session to be the
   helper's own session, and the endpoint descriptor to be exactly the form in item 1,
   owned by that host process's user SID. These checks bind the helper to a host of its
   own installation; they do not authenticate the host against other code running as
   the same user, which criterion 12 places outside the boundary. That residual is
   bounded: such code can supply at most that same user's token, and the helper
   re-verifies the source against the signature and the floor.
4. *The initiating token comes from that same process object.* From the pinned host
   process the helper opens the primary token with query, duplicate and assign-primary
   rights and duplicates it with `DuplicateTokenEx(TokenPrimary)`. It never derives the
   launch token from a thread or pipe impersonation token, from a token another process
   supplies, or from identity strings. It reads TokenUser, `AuthenticationId`, session
   id, elevation type and integrity level through `query_windows_peer_token_facts` (§5)
   and refuses an elevated or non-Medium initiating token here, before any protected
   write, because the later claim check would come after a version was consumed; it then
   records `initiating_logon`. Over-the-shoulder consent with alternate administrator
   credentials changes only the helper's own token; the initiating token still comes
   from the host process object, so that case is admitted. If slice S1 shows that the
   helper cannot open the host process or its token after alternate-administrator
   consent, T4d stops before slice S11 for a new owner decision, and until then that
   cell refuses with a typed `ProtectedStateUnchanged` whose detail names the
   unsupported consent, before any protected write. None of these is a fallback:
   enabling further privileges, changing the token's owner or DACL, a session-token API
   such as `WTSQueryUserToken`, or pipe impersonation.
5. *Source pinning uses token impersonation, not pipe impersonation.* The bootstrap
   request carries only KEL-53's bounded source lookup locator beneath the initiating
   user's owner-private staging root; it conveys no authority and cannot select an
   install root or write destination. The helper impersonates an impersonation-level
   duplicate of the initiating token on its own thread (`keld-guard`), resolves the
   locator through the Windows path owner, rejects reparse and substitution, opens the
   source files read-only, retains the exact file objects and denies concurrent
   write/delete where the platform permits. It reverts before any protected operation;
   a failed revert terminates the helper. It verifies the signed manifest and archive
   from those retained handles, copies into a protected sibling stage and reads back
   the exact bytes before the common single-writer transition (§4). It never reopens an
   untrusted source by path after pinning, and any source race or inability to pin
   stable bytes refuses before publication.
6. *Candidate launch.* The launch target is `CreateProcessWithTokenW` with the retained
   primary token and `CREATE_SUSPENDED`, only after native proof that the helper's
   token holds `SeImpersonatePrivilege` and that the helper runs in the initiating
   user's interactive session, because the new process runs in the caller's session,
   not the token's
   ([CreateProcessWithTokenW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createprocesswithtokenw),
   `ms.date` 2018-12-05). Every launch-readiness proof that needs no created process (the
   token facts, the session, `SeImpersonatePrivilege`, failed reversion, a zero
   `AuthenticationId` (owner decision 2026-10-06 (zero LUID refused)) or a zero logon
   time of the initiating session, and desktop and profile availability) runs before the
   mint-then-journal seam writes `PublishPending`,
   so its refusal is `ProtectedStateUnchanged`. Only process creation and the
   verification of the suspended process (its token, session, integrity and elevation,
   image, profile, environment and desktop, before it is resumed) come after
   `AwaitingHealth` is durable; their refusal terminates any suspended process and rolls
   the attempt back with `CandidateLaunch` under the same lease ("Candidate
   connect-back"). A crash inside that rollback leaves a phase that the journal treats
   as launched, so its recovery needs the owner-loss proof; the recovery-only role's
   abandon intent covers only attempts still in `PublishPending`. The helper never substitutes its elevated
   administrator token. `CreateProcessAsUser` is not an automatic fallback: its
   token-session behavior and additional caller privileges need separate exact proof. A
   pre-existing ordinary-user launcher may be reused only if its installed lifecycle and
   authenticated hand-off independently satisfy this contract. This qualification does
   not select or constrain the separate machine-seamless mechanism.

The bootstrap messages are listed once in "Candidate connect-back". Opening the host
process and its token from the elevated helper, including after alternate-administrator
consent (with the stop rule in item 4), the `hProcess` rights, and same-session
`CreateProcessWithTokenW` are qualification targets of T4d slice S1.

Mode is authenticated installation provenance, never inferred from Program Files paths,
ACL observations, executable location, command line, environment, or caller flags. The
same candidate bytes, floor, journal transitions, health predicate and rollback rules
apply to each direct mode. Only the lease-acquisition adapter and its native evidence
matrix vary.

`MachineSeamlessDirect` keeps KEL-270's approved bounded proof slice as a prerequisite:
exact attempt Job handle, bounded keeper, one-shot authenticated result transfer,
writer-lease/lifecycle coupling, and fail-closed recovery. No production pointer,
floor, activation, commit or rollback writes connect to that keeper slice until it
passes coordinator-death, keeper-death, wrong-Job/host, replay/stale-attempt, competing
writer, process-family-zero and ambiguous-reboot controls. A Windows Service, Scheduled
Task, installer helper or other mechanism is not preselected. If all-owners-lost or
reboot state cannot be proven, recovery preserves journal/pointers and halts.

The amendment expands acceptance coverage for AC1/6/7/8/10/12/14 and T4b/T5 into
independent per-user, machine-UAC, machine-seamless and managed-owner cells. The UAC
cell additionally tests request-to-user/session binding, exact signed helper identity,
initiating-token handle provenance and lifetime, TokenUser/AuthenticationId/session/
elevation/integrity validation, association to the initiating process, alternate UAC
credentials, unavailable/substituted token handles, one-shot source locator and
elevated-owner-opened read-only source handles,
source-file substitution/reparse/write races, independent signature/artifact
revalidation, replay and cross-install refusal, protected copy/readback, mutation
confinement, exact initiating-user candidate token, wrong-session refusal, process-family lifecycle,
helper/coordinator death at every persisted boundary, and the typed recovery-required
state with its recovery-only resolution. A positive control proves the exact authorized operation
can complete. Per-user tests assert same-user no-UAC activation and its stated threat
exclusion. Machine-UAC
tests assert explicit elevation only at protected activation and prove the application
never runs elevated. Machine-seamless tests remain refusal-only until KEL-270's proof
gate and selected mechanism pass. Managed-owner controls prove no direct writer. Each
cell runs the same durable state-machine trace and crash-cut matrix; a pass in one
authority cell does not close another.

## 2. Spec refs

- `docs/architecture/06-runtime-and-tooling.md` §4/§4a owns the v0 feed,
  manifest, canonical archive, verification order, local trust floor and activation
  model.
- `docs/architecture/03-security.md` owns update trust, protected state and the
  narrow Windows relaunch-helper boundary.
- `keld-pack` owns the Windows v0 package producer and exact no-migration policy
  path/bytes, and depends on `keld-guard` for the single Windows package-path
  validator. `keld-update` depends on both `keld-guard` and `keld-pack` so it can consume
  that policy owner; packaging never depends on the updater. The first Windows x64
  producer runs on a Windows host to use the guard-owned Windows namespace contract.
  Other hosts refuse before output. Cross-host assembly remains the target; this
  support cell expands only after independent evidence proves Windows-name equivalence.
- KEL-137 owns a future canonical package representation with executable modes and
  bundle links. It precedes every macOS/Linux package cell and any Windows package that
  cannot fit the current regular-file/directory-only v0 archive.
- KEL-53 owns activation, health binding, channel-owner refusal, helper confinement,
  fault injection and binary-versus-data honesty.
- KEL-130 owns the shared Windows lexical-component classifier. Before T3 its owner
  must be amended to include the complete package-required forbidden/control and 8.3
  policy; package validation consumes it and must not copy a second list.
- KEL-90/KEL-129 own measurements and budgets.
- PR #30 landed the current signed-manifest/feed wire contract.
- Microsoft documents token-based process launch in [`CreateProcessAsUser`](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessasusera)
  and [`Processes in the Client Security Context`](https://learn.microsoft.com/en-us/windows/win32/secauthz/processes-in-the-client-security-context).
  These platform contracts do not by themselves prove Keld's initiating-user token
  handoff or process lifecycle.
- The Windows UAC qualification candidate also depends on Microsoft's contracts for
  [`ImpersonateNamedPipeClient`](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-impersonatenamedpipeclient),
  [`DuplicateTokenEx`](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-duplicatetokenex),
  and [`CreateProcessWithTokenW`](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createprocesswithtokenw).
  These define token type, access-right and session constraints; Keld still requires
  exact Windows proof for request binding and lifecycle.

This approved corrected contract keeps the v0 JSON fields but intentionally revises their validation,
client-selection and canonical-package semantics before any updater implementation
exists. Approval of the corrected exact content head recorded above re-approves the v0 public
contract; a post-implementation
semantic change requires a new schema/version and wire review. The contract reuses the
existing strict semantic-version floor. There is no release-sequence, expiry, or
security-epoch field to validate. Adding one also requires a new manifest version.

Current durability inputs are Microsoft `FlushFileBuffers` and
`MoveFileExW(MOVEFILE_WRITE_THROUGH)`, POSIX synchronized I/O and directory
cache requirements, and Apple's `F_FULLFSYNC`. They define adapter order;
implementation still needs real-OS crash-cut evidence for each admitted filesystem.
Windows lease sharing and lock lifetime follow the documented [`CreateFileW` share
compatibility and handle-close rules](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew);
record replacement uses only the documented same-volume [`MoveFileExW` replace-existing/
write-through flags](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw). Explicit UAC owner
assignment follows Microsoft's [`TOKEN_OWNER` valid-owner-group rule](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_token_owner)
and [owner rights](https://learn.microsoft.com/en-us/windows/win32/secauthz/owner-of-a-new-object); the candidate still runs in the initiating user's ordinary session under
[UAC](https://learn.microsoft.com/en-us/windows/security/application-security/application-control/user-account-control/how-it-works).

## 3. Acceptance criteria (binary, each becomes a test)

1. Given a direct-distribution installation, startup consumes installer-created
   `keld.install-provenance/v2` binding the exact app id, channel, target, install root,
   update root, installed baseline artifact, compiled-in signing-key identity, explicit
   install mode and mode-specific OS protection profile. Mode-less v1 provenance is
   unsupported until an explicit trusted migration records one mode; location never
   supplies that choice. Mutation checks are evaluated against the
   selected mode's admitted writer authority: provenance changed by a principal outside
   that authority fails, while owning-user control of a `PerUserDirect` installation is
   an explicit threat exclusion. Direct modes are `PerUserDirect` (default,
   user-owned LocalAppData tree), `MachineUacDirect` (explicit Program Files mode with
   Administrators/SYSTEM write and ordinary-user/Keld-role read/execute), and
   `MachineSeamlessDirect` (opt-in machine mode with SYSTEM-protected state and a
   separately approved narrow coordinator). Package/deployment-managed ownership is
   recorded as `Managed(owner)` and refuses Keld direct mutation. Missing, mismatched,
   unsupported or legacy-profile provenance refuses before feed access or filesystem
   mutation. A mode change requires an explicit trusted transition. Paths or executable
   names never infer mode or channel owner.
   A per-user install is protected from restricted Keld roles, but not from its owning
   user or arbitrary native code already running as that same user. Machine-wide ACLs
   do not claim protection from administrators.
   Before publishing provenance, the installer durably seeds `version-floor`,
   `current` and `last-known-good` to that same baseline artifact/version;
   provenance is the transaction's final commit record. Once direct provenance exists,
   a missing/corrupt floor fails closed even before the first update.
2. Given `updates.json` and its detached signature, verification follows
   architecture 06's existing order: literal-byte signature; duplicate-member-rejecting
   JSON; recognized schema; exact app/channel/target; strict SemVer; release versions
   unique by SemVer precedence; exact-duplicate delta `fromVersion` rejection; canonical
   positive `size`/`contentSize` integers in `1..=9007199254740991`; and valid
   digests. Invalid signatures, an unrecognized schema value, duplicate JSON members,
   equal-precedence releases, identity mismatch or malformed fields return a typed
   actionable refusal and activate no bytes.
3. Given a valid manifest, the existing persisted semantic-version floor filters for
   versions with SemVer precedence strictly greater than the floor and the client selects
   the single highest eligible version while retaining its complete version string as
   artifact identity. If filtering leaves no eligible release, return a typed successful
   no-update result without downloading, staging or mutating update state. A signed
   release at or below the installed baseline is ineligible. Malformed manifests and a
   missing/corrupt floor under direct provenance still fail closed; rollback never lowers
   the floor.
4. The first slice always downloads `full`, requires compressed and decompressed byte
   counts to equal their declared bounds, verifies both digests, validates the entire
   canonical archive before extraction, and writes archive entries only beneath protected
   `<version>/tree/`; retained `content.tar` and `.complete` are
   sibling updater metadata. A present delta entry has no effect. Exact ustar
   numeric/checksum/padding and complete-directory-entry golden vectors bind
   every producer and consumer to byte-identical `contentBlake3` input.
5. The first package cell is Windows x64 direct distribution and admits only packages
   representable by v0: regular files/directories, no links or special files, and the
   exact canonical metadata already defined in architecture 06. macOS/Linux and any
   link- or executable-mode-dependent package remain unsupported until an approved
   KEL-137 artifact replaces that limitation. Before any Windows write, the shared
   guard-owned classifier rejects separators/prefixes, ADS, Win32
   forbidden/control characters, reserved devices, trailing dot/space and tilde/8.3
   alias-shaped names; names must already be NFC, and the complete entry set must be
   unique under Windows ordinal case-insensitive comparison with no ancestor collision.
   The first producer for this cell runs on Windows because this exact admission uses
   native Windows normalization/comparison. A non-Windows producer host returns a typed
   unsupported-host result before output; cross-host assembly remains the target and
   requires independent equivalence evidence before that restriction is removed.
6. Before changing the trust floor or runnable pointer, the owner durably writes one
   activation journal containing a fresh attempt id, exact candidate
   `(app, channel, target, version, contentBlake3)`, validated rollback target,
   exact prior floor, prior last-known-good and previous-known-good artifacts. The order is journal
   `publish-pending`, candidate version publication, trust floor, `current`, journal
   `awaiting-health`; each step is durable before the next begins. Until the journal is
   durable the candidate exists only as a completed `incomplete-*` stage.
7. After termination at every persisted boundary, recovery under the single-writer lock
   either resumes that exact journaled attempt, completes a journaled rollback, or halts
   for manual recovery. A malformed, replayed, mixed-artifact or pointer-inconsistent
   journal halts; directory presence and the floor never substitute for the journal.
   Recovery first proves the prior coordinator/candidate process family is gone. For
   `publish-pending`, current equal to candidate requires floor exactly equal to
   candidate and advances to `awaiting-health` without republishing. Current equal
   to rollback target permits only the recorded prior floor or exact candidate floor
   before resuming; every other combination, including floor above candidate, halts.
   At the prior floor, a still-staged candidate is published first, and a candidate
   that is neither published nor staged abandons the attempt without changing any
   record. At the candidate floor the candidate must already be published.
8. Candidate health is accepted only over a private channel whose identity `keld-update`
   mints inside the lease-holding attempt owner for the journaled attempt. The receipt
   binds the attempt id and the full artifact identity through the §4 receipt digest
   (approved: KEL-270 owner decision `eff8e2fb`, 2026-10-06); the
   candidate host
   must have booted from that exact version, reached application `Ready`, and
   remained alive for 30 monotonic seconds with no unexpected generation exit. A generic
   marker, prior receipt, different artifact, clean early exit, timeout, crash or lost
   channel cannot commit health.
   Candidate boot takes no authority from argv or environment and is admitted only when
   the live attempt owner accepts, over its one-shot connect-back endpoint, the exact
   process that it launched and still retains; §4 "Candidate connect-back" is the single
   owner of the endpoint, claim, acceptance and refusal rules. The accepted candidate
   enters read-only candidate mode: it verifies journal/current identity, does not
   acquire the writer lock or run orphan recovery, and cannot self-commit health.
9. The previous last-known-good pointer and package remain unchanged until exact health
   is durably recorded. The owner then journals `health-accepted`, moves the prior
   last-known-good to `previous-known-good`, publishes `last-known-good` to
   the candidate, retires the superseded older version (if any), and removes the journal.
   Failure journals `rollback-pending`, republishes `current` to the attempt's
   validated rollback target, retires the failed candidate, and only then removes the
   journal. Neither path lowers the trust floor; bounded cleanup retains both
   known-good slots.
10. If Windows requires a post-exit helper, the signed helper inherits only protected
    update-root/lock handles and the host-process wait handle; it inherits no endpoint.
    It is not used in `MachineUacDirect`: there it would have no initiating user SID or
    token input, and its own `helper_image_blake3` would break the recovery role's
    self-anchor. It reads the exact attempt from the protected journal and waits for
    that host to exit. It then continues the attempt as a resumed owner through the
    mint-then-journal seam, in which `keld-update` mints new channel identities, the
    helper creates and holds its own endpoint under "Candidate connect-back", and only
    then does the durable re-mint record, with its own owner fields, reveal the name. It
    performs the journaled same-volume publish, launches only the journaled executable,
    observes exact health, commits or rolls back, and exits.
    Command line, environment, cwd, caller paths and feed bytes convey no authority. The
    journal binds the verified helper image and health-channel identities; replay,
    endpoint substitution/reuse, helper substitution, mixed artifact set or path
    substitution fails.
11. Windows stages each complete version in a unique sibling directory, flushes every
    file handle, closes stage handles, then publishes the absent final directory with
    same-volume `MoveFileExW(MOVEFILE_WRITE_THROUGH)`. Journal/pointer records use
    same-directory temporary files, file flush and replace/write-through. Neither path
    uses copy-across-volume or claims a directory-handle flush. The implementation
    reopens and reads back every tree digest, journal, pointer and policy before
    advancing, and real crash cuts qualify the filesystem. Linux later uses file
    `fsync`, same-filesystem rename and parent-directory
    `fsync`; macOS later adds `F_FULLFSYNC` before rename plus
    directory synchronization. An unsupported barrier, remote filesystem or failed
    read-back makes that cell unsupported.
12. In every admitted direct mode, hostile app roles and webviews cannot write
    provenance, trust root, version floor, journal, staged package, pointers, helper
    input or install tree. The per-user updater and application host may share the
    ordinary user's OS identity; strict role restrictions must still deny Keld roles
    access to the user-owned updater state. Legacy same-user role mode refuses direct
    update because its token cannot be distinguished from the host. Administrators and
    arbitrary same-user native malware remain outside this boundary.
13. Every Slice-A package contains `.keld/update-policy.v1` with exact UTF-8
    bytes `{"schema":1,"dataMigration":"none"}\n`, covered by
    `contentBlake3`. Missing, duplicate or different policy refuses
    activation. Keld exposes no migration hook in this slice.
14. Disk-full, locked-file, interference, offline, corrupt download, signature failure,
    failed health, cleanup failure and concurrent-attempt fixtures preserve one
    diagnosable state. A failure before the version floor advances may retry the same
    signed version after repair. Once the floor advances, a launch/health-failed
    candidate remains ineligible for automatic reselection; a later automatic attempt
    requires a newly signed higher version. Removing all delta code/dependencies leaves
    the full-package updater complete.
15. A future delta path must verify reconstructed bytes against the selected release's
    `full.contentBlake3`, fall back once to `full` in the same
    attempt where safe, and retain the same journal, health, trust-floor and rollback
    contracts.
16. `PerUserDirect` is the default Windows installer choice. Installation and update
    state live under that user's LocalAppData-owned tree; the normal updater obtains its
    exclusive write lease as that ordinary user without UAC. Keld roles remain unable to
    write the state. Setup records the owner and mode before direct admission, provisions
    the stable activation-lease file with the owner-private descriptor, and seeds the
    same baseline/floor/current/LKG invariants as machine installation.
17. `MachineUacDirect` uses an installer-provisioned Program Files protection profile
    owned by BUILTIN Administrators, with a protected DACL granting full control only to
    BUILTIN Administrators and SYSTEM and read/execute (`0x1200A9`) to ordinary
    BUILTIN Users/Keld roles. The installer must be able to assign Administrators as a
    valid owner group in its actual token (`SE_GROUP_OWNER`, not deny-only); otherwise
    installation fails without owner/ACL takeover or repair. It provisions the stable
    activation-lease file and protected ancestors with this profile. Each activation that needs protected mutation requests UAC for a
    fixed signed updater helper; denial/cancellation causes zero protected writes. The
    helper is a dedicated minimal `keld-updater-helper.exe`, never an elevated
    `keld-host.exe`, launched and self-anchored as §4 "Helper launch and self-anchor"
    requires. Its Authenticode signer must equal the protected provenance
    publisher scope, its exact image is covered by the verified selected artifact's
    `contentBlake3`, and the journal's `helper_image_blake3` records its image digest. It contains only
    authenticated bootstrap, source pinning, protected activation, candidate launch,
    health/rollback ownership and the recovery-only role: no WebView, Bun or app runtime, feed or
    network client, arbitrary filesystem API, shell execution or general broker. This
    UAC helper is the Machine-UAC authority adapter for the common verifier and
    transaction: it authenticates its bootstrap from the admitted host ("Machine-UAC
    bootstrap"), obtains the lease, and independently revalidates the ordinary user's bounded cache input under
    retained read handles before any protected write. A user-side `VerifiedFull`
    receipt is not privileged proof. The production stage entry consumes an opaque
    mode-bound activation-write capability from this authenticated helper after it has
    freshly reloaded protected provenance and floor; a read-only `LoadedWindowsBaseline`
    or logical `AdmittedInstallation` alone cannot authorize protected staging. The
    helper creates a fresh protected sibling stage
    through the shared extraction/copy/readback pipeline; every directory and file has
    the exact Machine-UAC owner/DACL at creation, before payload bytes are written.
    It must not rename a user-owned stage into Program Files or seal a user-writable
    stage after the fact. Fake host, stale attempt, changed source bytes, a source with
    writable handles/mappings, or an unauthenticated endpoint fails before protected
    publication. Caller paths/argv/environment do not authorize mutation. The optional
    post-exit helper of criterion 10 is not used in this mode. The UAC helper owns the attempt while alive and launches the exact
    candidate under the initiating ordinary user's token/session (including
    over-the-shoulder approval), retaining exact-health/commit/rollback rules. The helper
    itself owns the attempt; a keeper is added only if the design proves it necessary, and
    then it is minimal, bound to one installation and one attempt, holds only attenuated
    exact handles and never a handle to the attempt Job, which only the launching
    component holds, exposes no mutation, process or filesystem command, and exits when the
    attempt is terminal. There is no persistent or ambient privileged broker. If the
    helper exits, or Windows restarts or every owner is lost before resolution, an
    ordinary process never recovers: it returns the typed
    `ActivationEffect::MachineRecoveryRequired` state, and only the helper's
    recovery-only role resolves it (§4 "Machine-UAC recovery-required state and
    recovery-only role" owns both).
    The helper's bootstrap, in which the host is the server and the helper the client
    ("Machine-UAC bootstrap"), and its health exchange use the separate versioned
    `keld-attempt` subprotocol of §4 "Candidate connect-back"; it reuses only the low-level framing,
    nonce, deadline and peer-process verification utilities, and the lifecycle purpose
    tags keep their lifecycle and keeper meaning. The app and Bun roles never run
    elevated or as SYSTEM.
18. `MachineSeamlessDirect` is an explicit install-time opt-in. It preserves the same
    updater transaction and requires its coordinator to authenticate exact host,
    installation and fresh attempt; prevent replay; acquire installation-wide exclusive
    writer ownership; bind candidate health and process-family lifecycle; launch the
    application as the ordinary user; and recover safely across crash/reboot. The
    product mode is approved, but the Windows authority mechanism is not. Task Scheduler,
    a service or another Windows-native mechanism may be selected only after each named
    negative and positive control passes. A task start/trigger is never update
    authorization. No implementation of a privileged coordinator is authorized by this
    criterion alone. Its trusted initializer seeds the stable activation lease with the
    SYSTEM profile.
19. `Managed(owner)` provenance makes `keld-update` return a typed owner-delegation
    result before fetching, staging or mutating. MSIX/App Installer, Store and enterprise
    deployment remain the only update authorities for their managed installation.
20. Each direct mode uses one trusted-installer-created regular `activation.lock` file
    under the protected update root. It is persistent, never deleted/recreated/replaced,
    and its presence does not identify a live owner; do not reuse KEL-266's
    `bootstrap.lock`. A writer opens the existing file read/write with exclusive sharing
    (share mode zero), no reparse following or inheritance. A snapshot reader opens it
    read-only with `FILE_SHARE_READ` only and holds that short lease for one coherent
    mutable-record snapshot. Normal startup retains only immutable selected-tree and
    ancestry pins after releasing mutable journal/floor/current/LKG record pins. The
    writer retains the exclusive lease while the candidate performs its authenticated
    bootstrap read; the candidate closes mutable-record pins and acknowledges bootstrap
    before application execution, retaining only immutable selected-tree pins and its
    owner connection through the 30-second window. That authenticated candidate-boot read,
    which happens only after the owner accepted the claim and while the owner replaces no
    mutable record, is the only reader exception. Every other sharing conflict is a typed
    busy/refusal (`WriterActive`) with no retry, sleep or record read, including any
    launch during an attempt that is not the accepted claimant (owner decision D3).
    Missing, wrong-kind, wrong-volume or wrong-profile lock state fails closed.
    On Windows machine-wide profiles, ordinary-user read access to this lease also lets
    a local native process hold a conflicting share-mode handle and deny update
    availability until that handle closes. The same read access to the mutable records
    lets such a process hold a handle that makes a writer's record replacement fail,
    because a file whose name is the replacement target cannot be replaced while it has
    open handles
    ([FILE_RENAME_INFORMATION](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_file_rename_information),
    `ms.date` 2021-11-22); a replacement that fails mid-transaction leaves the journal
    authoritative, so in `MachineUacDirect` every user sees `MachineRecoveryRequired`
    until the recovery-only role resolves it. Both are availability-only residuals: the
    updater must fail closed before or at the refused protected write, and no integrity
    claim depends on successful lease acquisition or replacement. The contract does not
    promise update availability against hostile local native users. This follows from
    the documented
    [CreateFile sharing rules](https://learn.microsoft.com/windows/win32/api/fileapi/nf-fileapi-createfilew).

## 4. Design

### Atomic decomposition and first-principles decision

| Atom / owner | Boundary and input → output | Failure mode | Independent observable |
|---|---|---|---|
| Feed wire / architecture 06 | signed v0 bytes → one highest eligible full release | equal-precedence releases, noncanonical/unbounded sizes or ambiguous parsing | immutable manifest bytes/signature fixtures with SemVer and numeric boundary mutations |
| Channel provenance / installer + host | protected receipt → direct admission or refusal | path heuristic mutates managed install | substitution table before feed/write counters |
| Package / `keld-pack` | full artifact → one Windows v0 tree/policy | unsupported link/mode or hidden migration | canonical bytes and hostile archive corpus |
| Trust / `keld-update` | verified version → monotonic semver floor | rollback lowers/bypasses floor | floor trace independent of runnable pointer |
| Activation / `keld-update` | candidate + prior LKG → commit or rollback | partial publish or directory inference | crash cut after every durable transition |
| Health / candidate host | private attempt channel → exact receipt | stale/generic/mixed receipt commits | field substitutions and kill/early-exit controls |
| OS durability / adapter | bytes + barriers → durable read-back | prerequisite missing after crash | native barrier failure and crash-cut matrix |
| Helper / Windows package | journal + inherited handles → post-exit publish | arbitrary path, replay or mixed set | independently substitute every helper input |
| User data / package owner | signed no-migration policy → rollback eligibility | binary rollback after migration | absent/changed policy refuses pre-launch |
| Evidence / task owner | exact source + OS receipts → task artifact | mock/stale head closes native row | exact-head provenance validator |
| Install mode / trusted installer + KEL-53 | explicit mode choice → immutable owner/protection provenance | path/environment selects authority | substitute path/env/registry; refuse before feed/write |
| Writer authority / mode adapter + `keld-update` | one exact attempt → one temporary exclusive write lease | UAC/start trigger mistaken for attempt authentication or concurrent writer admitted | wrong authority and competing writer leave journal/pointers unchanged |

The exact portable `.keld/update-policy.v1` path and bytes are owned once by `keld-pack`;
`keld-update` depends on that producer-side owner and checks the authenticated archive
entry byte-for-byte without JSON reserialization. An independent literal golden vector
prevents producer/consumer agreement on an incorrect constant. `keld-pack` has no
dependency on `keld-update`.

The manifest authenticates candidate bytes but does not own channel provenance, health or
local recovery. The trust floor decides future eligibility but never says which binary
is healthy. The journal owns only the in-flight attempt; `current`,
`last-known-good` and `previous-known-good` own their selections.

**Reuse:** keep the v0 detached signature, strict semver floor, canonical tar, version
directories and single-writer lock. Slice A ignores delta entries and adds no dependency
or manifest field.

**Rejected alternatives:** a generic health marker can outlive its candidate; advancing
last-known-good before health destroys the rollback target; path inference can mutate a
store-owned install; directory reconstruction accepts partial/mixed attempts; lowering
the floor reopens replay; an un-hashed policy permits substitution.

Compatibility fallback: unsupported packaging/channel/filesystem cells keep the current
version and report the missing predecessor or owning update mechanism.

**Target boundary:** `keld-update` remains the common transaction and recovery owner,
and it alone mints attempt, health and lifecycle identities, inside the lease-holding
attempt owner; config, roles, environment and the feed never supply them (landed
`activate.rs:480`, `:697`). That owner creates the connect-back endpoint and the
candidate only connects back to it ("Candidate connect-back"). The
mode-specific adapter obtains the one temporary write lease for that attempt: the
ordinary user-owned updater for `PerUserDirect`, an explicitly elevated signed helper
for `MachineUacDirect`, or a still-unselected narrow coordinator for
`MachineSeamlessDirect`. `Managed(owner)` obtains no Keld write lease. Every adapter
executes the same journal, floor, pointer, health, commit, rollback and crash-recovery
transitions. UAC/task/service wake-up is not attempt authentication. A helper may own a
live attempt only for the bounded health/commit/rollback lifecycle, with identities that
`keld-update` mints inside it; it cannot supply identity of its own or survive as a
general updater.

**Cross-owner contract:** KEL-135 owns verified publisher/app/profile identity. KEL-53
alone mints `ActivePackageSelection` from journal/current/LKG state and owns install-mode
provenance, update journal and the sole writer. KEL-254 owns OS verification of
installed-image and protection profiles but never selects current/LKG or mints an active
selection. KEL-96/core consumes that exact `ActivePackageSelection`, performs host boot
admission and returns its opaque `ValidatedBootSelection` to the ordinary-user host.
None of the consumer/identity owners infers install mode or mutates updater state.
KEL-254's approved installed-root spec defines the per-user and Machine-UAC protection
profiles; their native implementation and verification remain required before those
direct modes ship.

### Internal state and transition contract

These internal shapes are not public Rust API or manifest wire, except the owner-fact
value types `AttemptOwner` and `InitiatingLogon`: the attempt owner constructs them,
with private fields, to call `WindowsMintedAttempt::journal`, so they are public API
(§8; amended for the KEL-270 T4d S3 independent public-API review):

```rust
struct ArtifactIdentity {
    app_id: CanonicalAppId,
    channel: Channel,
    target: Target,
    version: StrictSemver,
    content_blake3: [u8; 32],
}

/// One durable record whose context every phase shares, including `RollbackPending`.
/// Landed as `keld-update`'s `records::ActivationJournal`, schema
/// `keld.activation-journal/v1`; the T4d fields below revise it to v2.
struct ActivationJournal {
    attempt_id: [u8; 32],
    candidate: ArtifactIdentity,
    rollback_target: ArtifactIdentity,
    prior_floor: StrictSemver,
    prior_last_known_good: ArtifactIdentity,
    prior_previous_known_good: Option<ArtifactIdentity>,
    // The verified executable that owns protected publication and candidate launch:
    // the host coordinator or a criterion-10 post-exit helper (`PerUserDirect`), or
    // `keld-updater-helper.exe` (`MachineUacDirect`).
    helper_image_blake3: [u8; 32],
    health_channel_id: [u8; 32],
    lifecycle_channel_id: [u8; 32],
    // T4d schema revision `keld.activation-journal/v2` (wire-gated): both owner facts
    // are required in every v2 record and phase. One `Option` holds both, so a record
    // with only one is unrepresentable. `None` models the decoding of a v1 record, which
    // has neither and therefore never admits a candidate claim, and the in-memory
    // identities the mint-then-journal seam minted before `WindowsMintedAttempt::journal`
    // supplies both facts; that minted state is never encoded.
    ownership: Option<AttemptOwnership>,
    phase: ActivationPhase,
}

/// The v2 owner facts of one attempt; the wire encodes them as two sibling objects.
struct AttemptOwnership {
    initiating_logon: InitiatingLogon,
    attempt_owner: AttemptOwner,
}

/// The process that creates the connect-back endpoint and launches the candidate (§4
/// "Candidate connect-back").
struct AttemptOwner {
    owner_process_id: u32,
    owner_creation_time: u64, // `GetProcessTimes` creation FILETIME; zero refuses launch
}

/// The initiating process token's logon session ("Machine-UAC owner-loss retirement").
struct InitiatingLogon {
    authentication_id: Luid, // `TokenStatistics.AuthenticationId`; zero refuses launch
    logon_time: i64,         // `SECURITY_LOGON_SESSION_DATA.LogonTime`; zero refuses launch
}

enum ActivationPhase {
    PublishPending,
    AwaitingHealth,
    HealthAccepted { health_receipt_digest: [u8; 32] },
    RollbackPending { failure: ActivationFailureClass },
}
```

There is one journal struct, not one per phase: `RollbackPending` adds only its closed
failure class and keeps the whole shared context, including `initiating_logon`, so a
rollback interrupted by owner loss is retired by the same logon-session proof as the
attempt it rolls back.

The v2 canonical JSON keeps the landed record style (`records.rs`: typed wire struct,
unknown fields denied, and decoding accepted only when re-encoding reproduces the exact
bytes). It adds `"initiating_logon": {"authentication_id": h16, "logon_time": h16}` and
`"attempt_owner": {"owner_process_id": n, "owner_creation_time": h16}`, where `h16` is a
string of exactly 16 lowercase hexadecimal digits of an unsigned 64-bit value, and `n`
is a JSON integer in `1..=4294967295`. `authentication_id` is the LUID as
`(HighPart << 32) | LowPart`, nonzero by owner decision 2026-10-06 (zero LUID refused)
("Machine-UAC owner-loss retirement" fact 2); `logon_time` and `owner_creation_time`
are the FILETIME values, nonzero, and a negative `LogonTime` refuses. The wire review
fixes these golden vectors. The two objects follow `lifecycle_channel_id` and precede
`phase`, in the order above, under schema `keld.activation-journal/v2`. The fixture's
logon session has `HighPart` 1 and `LowPart` `0x0002a5f3`, its `LogonTime` is
2026-10-06T08:00:00Z, and its owner is process 4242, created 42 seconds later. The exact
accepted records, one per phase, are checked in as
`crates/keld-update/src/records/golden/journal-v2-<phase>.json`. Each is one line with no
trailing newline; they differ only in `phase`, as in v1.

`journal-v2-publish-pending.json`:

```json
{"schema":"keld.activation-journal/v2","attempt_id":"1111111111111111111111111111111111111111111111111111111111111111","candidate":{"app_id":"dev.keld.fixture","channel":"stable","target":"windows-x64","version":"1.1.0","content_blake3":"4444444444444444444444444444444444444444444444444444444444444444"},"rollback_target":{"app_id":"dev.keld.fixture","channel":"stable","target":"windows-x64","version":"1.0.0","content_blake3":"0101010101010101010101010101010101010101010101010101010101010101"},"prior_floor":"1.0.0","prior_last_known_good":{"app_id":"dev.keld.fixture","channel":"stable","target":"windows-x64","version":"1.0.0","content_blake3":"0101010101010101010101010101010101010101010101010101010101010101"},"prior_previous_known_good":{"app_id":"dev.keld.fixture","channel":"stable","target":"windows-x64","version":"0.9.0","content_blake3":"3333333333333333333333333333333333333333333333333333333333333333"},"helper_image_blake3":"5555555555555555555555555555555555555555555555555555555555555555","health_channel_id":"6666666666666666666666666666666666666666666666666666666666666666","lifecycle_channel_id":"8888888888888888888888888888888888888888888888888888888888888888","initiating_logon":{"authentication_id":"000000010002a5f3","logon_time":"01dd5568af7ac000"},"attempt_owner":{"owner_process_id":4242,"owner_creation_time":"01dd5568c8837100"},"phase":{"phase":"publish-pending"}}
```

`journal-v2-awaiting-health.json`:

```json
{"schema":"keld.activation-journal/v2","attempt_id":"1111111111111111111111111111111111111111111111111111111111111111","candidate":{"app_id":"dev.keld.fixture","channel":"stable","target":"windows-x64","version":"1.1.0","content_blake3":"4444444444444444444444444444444444444444444444444444444444444444"},"rollback_target":{"app_id":"dev.keld.fixture","channel":"stable","target":"windows-x64","version":"1.0.0","content_blake3":"0101010101010101010101010101010101010101010101010101010101010101"},"prior_floor":"1.0.0","prior_last_known_good":{"app_id":"dev.keld.fixture","channel":"stable","target":"windows-x64","version":"1.0.0","content_blake3":"0101010101010101010101010101010101010101010101010101010101010101"},"prior_previous_known_good":{"app_id":"dev.keld.fixture","channel":"stable","target":"windows-x64","version":"0.9.0","content_blake3":"3333333333333333333333333333333333333333333333333333333333333333"},"helper_image_blake3":"5555555555555555555555555555555555555555555555555555555555555555","health_channel_id":"6666666666666666666666666666666666666666666666666666666666666666","lifecycle_channel_id":"8888888888888888888888888888888888888888888888888888888888888888","initiating_logon":{"authentication_id":"000000010002a5f3","logon_time":"01dd5568af7ac000"},"attempt_owner":{"owner_process_id":4242,"owner_creation_time":"01dd5568c8837100"},"phase":{"phase":"awaiting-health"}}
```

`journal-v2-health-accepted.json`:

```json
{"schema":"keld.activation-journal/v2","attempt_id":"1111111111111111111111111111111111111111111111111111111111111111","candidate":{"app_id":"dev.keld.fixture","channel":"stable","target":"windows-x64","version":"1.1.0","content_blake3":"4444444444444444444444444444444444444444444444444444444444444444"},"rollback_target":{"app_id":"dev.keld.fixture","channel":"stable","target":"windows-x64","version":"1.0.0","content_blake3":"0101010101010101010101010101010101010101010101010101010101010101"},"prior_floor":"1.0.0","prior_last_known_good":{"app_id":"dev.keld.fixture","channel":"stable","target":"windows-x64","version":"1.0.0","content_blake3":"0101010101010101010101010101010101010101010101010101010101010101"},"prior_previous_known_good":{"app_id":"dev.keld.fixture","channel":"stable","target":"windows-x64","version":"0.9.0","content_blake3":"3333333333333333333333333333333333333333333333333333333333333333"},"helper_image_blake3":"5555555555555555555555555555555555555555555555555555555555555555","health_channel_id":"6666666666666666666666666666666666666666666666666666666666666666","lifecycle_channel_id":"8888888888888888888888888888888888888888888888888888888888888888","initiating_logon":{"authentication_id":"000000010002a5f3","logon_time":"01dd5568af7ac000"},"attempt_owner":{"owner_process_id":4242,"owner_creation_time":"01dd5568c8837100"},"phase":{"phase":"health-accepted","health_receipt_digest":"7777777777777777777777777777777777777777777777777777777777777777"}}
```

`journal-v2-rollback-pending.json`:

```json
{"schema":"keld.activation-journal/v2","attempt_id":"1111111111111111111111111111111111111111111111111111111111111111","candidate":{"app_id":"dev.keld.fixture","channel":"stable","target":"windows-x64","version":"1.1.0","content_blake3":"4444444444444444444444444444444444444444444444444444444444444444"},"rollback_target":{"app_id":"dev.keld.fixture","channel":"stable","target":"windows-x64","version":"1.0.0","content_blake3":"0101010101010101010101010101010101010101010101010101010101010101"},"prior_floor":"1.0.0","prior_last_known_good":{"app_id":"dev.keld.fixture","channel":"stable","target":"windows-x64","version":"1.0.0","content_blake3":"0101010101010101010101010101010101010101010101010101010101010101"},"prior_previous_known_good":{"app_id":"dev.keld.fixture","channel":"stable","target":"windows-x64","version":"0.9.0","content_blake3":"3333333333333333333333333333333333333333333333333333333333333333"},"helper_image_blake3":"5555555555555555555555555555555555555555555555555555555555555555","health_channel_id":"6666666666666666666666666666666666666666666666666666666666666666","lifecycle_channel_id":"8888888888888888888888888888888888888888888888888888888888888888","initiating_logon":{"authentication_id":"000000010002a5f3","logon_time":"01dd5568af7ac000"},"attempt_owner":{"owner_process_id":4242,"owner_creation_time":"01dd5568c8837100"},"phase":{"phase":"rollback-pending","failure":"health-rejected"}}
```

| Field | Accepted | Refused (`journal-v2-refusals.txt` beside the goldens) |
|---|---|---|
| `authentication_id` | `0000000000000001` to `ffffffffffffffff` | `0000000000000000` (owner decision 2026-10-06 (zero LUID refused)); uppercase, 15 or 17 digits, a non-hex digit, a `0x` or sign prefix, a JSON number, an escaped digit |
| `logon_time` | `0000000000000001` to `7fffffffffffffff` | `0000000000000000`; `8000000000000000` to `ffffffffffffffff`, the two's complement of a negative `LogonTime`; uppercase, 15 digits, a JSON number |
| `owner_process_id` | `1` to `4294967295` | `0`, `4294967296`, `-1`, `4242.0`, `4.242e3`, `04242`, `"4242"` |
| `owner_creation_time` | `0000000000000001` to `ffffffffffffffff` | `0000000000000000`; uppercase, 17 digits, a non-hex digit |
| record shape | both objects in v2, neither in v1 | an object missing or `null` in v2; either present in v1; another schema; swapped objects or keys, an object after `phase`, whitespace; a key missing from either object; a duplicate or unknown key at either level |

A v1 record still decodes, with neither object, and admits no claim. The phase writes
that finish a decoded v1 attempt keep its v1 encoding, because no owner facts exist for
it. Each record that first reveals minted identities, `PublishPending` for a fresh
attempt or the re-mint record for a resumed one, carries the writing owner's own
`attempt_owner` and `initiating_logon` and is therefore v2.

The journal is a strict versioned local record. Unknown versions, duplicate fields,
noncanonical values and pointer/artifact mismatches fail closed. `keld-update` mints the
random `attempt_id`, like the channel identities, inside the lease-holding attempt
owner; config, roles, environment and feed cannot supply it.
The journal also carries a fresh `lifecycle_channel_id`, distinct from both the
attempt and health identities. A stable rendezvous locator is derived from trusted,
immutable installation identity and user/session scope, so a cold successor can
discover the keeper without reading mutable journal data while the keeper holds the
share-zero lease. The locator is not authorization and MUST NOT disclose secrets. After
peer authentication, the keeper supplies the bound attempt ID and both sides bind a
one-use challenge/ack to install, attempt, peer process and connection generation.

The lifecycle `installation_id` is owned by `keld-update`; callers MUST NOT mint it
from a path, random value, command-line value or separate installer assertion. The
updater derives its 32 bytes as
`BLAKE3(UTF8("keld.installation-binding/provenance-v2/v1\0") || u64_le(n) || p)`,
where `p` is the exact canonical encoded `keld.install-provenance/v2` record and `n`
is its byte length. This binds the explicit mode, owner, app/channel/target, install and
update roots, signing-key identity, baseline artifact, profile, principal model,
publisher scope and volume. The running host derives the expected ID from trusted
packaging configuration; recovery recomputes it only after reading protected provenance
and matching every field to that configuration. On the KEL-254 executable-located path
the host derives it only from provenance admitted under that path's anchor. Equal
provenance yields a stable ID;
relocation changes the ID only when the admitted canonical provenance bytes change (a
reinstall), because the executable-located selector refuses a record whose roots differ
from the located roots. This digest is binding context, not a secret or peer
authentication. A future provenance schema MUST preserve an explicitly defined v2
projection or introduce a separately versioned lifecycle-ID derivation.

Before any Job witness or lease-retention capability moves, each side MUST authenticate
the actual connected named-pipe peer and same-session profile. The health token is
never reused for keeper recovery or exposed to app roles.

The lifecycle rendezvous MUST NOT treat the existing bearer-token `HELLO`, a pipe DACL,
PID, SID or signed publisher alone as peer authentication. Both ends retain the actual
connected pipe and process objects, verify exact permitted role/image plus token user,
session and integrity, bind the independently expected install and attempt, and reject
exit or identity changes during acquisition. The client sends a fresh nonce; the server
returns a fresh challenge and the client acknowledges the full transcript. These values
prove freshness/liveness only; OS peer identity authenticates. The client MUST set
`SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION` when opening its pipe to the server.
The server validates the last HELLO writer's thread token through
`ImpersonateNamedPipeClient`/`TOKEN_QUERY`, always reverting impersonation. Client and
server pipe handles MUST be non-inheritable
and MUST NOT be duplicated to app roles. If the connected endpoint can be inherited or
duplicated to an untrusted role, PID queries do not establish who wrote the message and
the mechanism refuses to transfer handles. A one-shot generation is consumed before
any capability transfer; simultaneous successors, stale attempt IDs and transcript
replay all refuse. If `RevertToSelf` fails, the process MUST terminate immediately and
MUST NOT return to the accept/retry path.

The lifecycle pipe uses the dedicated `\\.\pipe\keld-lifecycle-<64 lowercase hex>`
namespace, disjoint from ordinary `\\.\pipe\keld-<64 lowercase hex>` app-link pipes.
The two clients MUST reject the other namespace before opening a pipe; the endpoint
prefix is the protocol discriminator before the distinct nonce handshake. After the
client/server nonce exchange, the server sends one fixed-size 177-byte `KELD-LC1`
binding challenge; the client returns `KELD-LA1` only after validating it against its
independent expectation. The server consumes its one-shot listener before sending the
same-context `KELD-LR1` acceptance receipt. The server returns an admitted peer only
after the receipt write completes before its deadline; the client returns only after
verifying that receipt before its deadline. The record is: 8-byte magic,
1-byte purpose, 32-byte
installation ID, 32-byte attempt ID, 32-byte lifecycle-channel ID, 32-byte client
nonce, 32-byte server nonce, 4-byte client PID and 4-byte server PID (PIDs little
endian). Purpose is a closed tag for coordinator-to-keeper or keeper-to-successor.
The client MUST compare install and purpose before acknowledgement. A cold successor
may learn attempt/channel only from the authenticated keeper, and MUST mark those IDs
as unverified until it reacquires the writer lease and revalidates them against the
protected journal; it MUST NOT mutate journal or pointers on a mismatch. Both peers
MUST validate the actual connected process objects and fresh nonce pair. Mismatch,
wrong purpose, stale exact-attempt expectation, malformed/truncated challenge or
acknowledgement/receipt MUST fail closed before any handle transfer. `LC1`/`LA1`/`LR1` version this
dedicated lifecycle subprotocol; ordinary app-link frames are unchanged.

After LR1, the coordinator sends one exact 89-byte `KELD-HO1` bundle on the same
consumed connection: 8-byte magic, coordinator-to-keeper purpose byte, 32-byte
attempt ID, 32-byte lifecycle-channel ID, 8-byte target-process Job handle and
8-byte target-process activation-lock handle (handle values little endian). The
duplicated Job has only `JOB_OBJECT_QUERY | JOB_OBJECT_TERMINATE`; the lease handle
has only `FILE_READ_ATTRIBUTES | SYNCHRONIZE`. The keeper adopts the exact process
handles, re-attenuates both, rejects inherited/wrong-object handles, and independently
queries a nonzero Job family before sending the exact 76-byte `KELD-HR1` receipt
(magic, attempt ID, channel ID, active count). The coordinator retains its local Job
and writer-lease owners until HR1 matches; a failed transfer keeps local ownership
and halts. If a malformed/truncated HO1 leaves remote handle values unknown, the
dedicated keeper MUST terminate before returning to any caller so process teardown
closes those handles. After delivery may have begun, the coordinator MUST NOT close
remote handles by their saved numeric values: the keeper may already have closed and
reused those slots. Instead it terminates and waits for the exact pinned keeper while
retaining its own Job/lease handles.

After the pinned coordinator exits, the keeper terminates the exact Job and MUST
observe zero before sending a query witness over the one-shot successor connection. Its 81-byte
`KELD-QO1` record contains magic, keeper-to-successor purpose, attempt ID, channel ID
and the remote query-only Job handle. The successor independently queries zero and
sends a 76-byte `KELD-QA1` receipt with the same attempt/channel and zero count. Only
after exact QA1 does the keeper close its activation-lock retention and send the
matching `KELD-QF1` final receipt. The successor returns from the lifecycle API only
after QF1, then acquires the writer lease and revalidates install, journal attempt
and channel before mutation. Before QA1 the keeper MUST retain the lock and halt on
failure. Observing Job zero alone does not authorize lease release: the successor
must cross the explicit QA1 gate. If QA1 succeeds but QF1 delivery is lost, the
successor MUST still halt without journal/pointer writes; another writer without the
retirement witness also refuses. The records carry handles only over the authenticated, one-use connection;
PID/name discovery never substitutes for a retained process object or Job handle.

Primary API contracts: [GetNamedPipeClientProcessId](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeclientprocessid),
[GetNamedPipeServerProcessId](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeserverprocessid),
[ImpersonateNamedPipeClient](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-impersonatenamedpipeclient),
and [DuplicateHandle](https://learn.microsoft.com/en-us/windows/win32/api/handleapi/nf-handleapi-duplicatehandle).

**Lease and snapshot ownership:** every direct installation receives a trusted-installer-
created stable `activation.lock` regular file under `update_root`. The file is never
deleted/recreated/replaced; existence is not live-owner evidence. A writer opens this
existing object read/write with share mode zero, no reparse following or inheritance.
Normal selection/recovery readers open read-only with `FILE_SHARE_READ` only and hold
that shared lease only while reading one coherent set of mutable records. A sharing
conflict is busy/refusal, without sleep or retry. The writer lease remains held through
the active attempt and health window. Criterion 20 owns the only reader exception, the
authenticated candidate-boot read of the accepted claimant, and every other conflict's
typed `WriterActive` refusal.
Normal host selection closes mutable journal/pointer/floor pins after snapshot and keeps
only the selected immutable artifact pins. Lock presence never decides whether the
previous coordinator/candidate family is live; recovery still requires the independent
process-family owner. `bootstrap.lock` remains only the KEL-266 initializer marker.

The process-family owner is an independent input to the common transaction: an
activation lease or a valid journal cannot prove an earlier host/candidate is gone.
On Windows, KEL-96/runtime owns ordinary-user host admission and process-family
observation; KEL-53 consumes that exact result before recovery or replacement. The
current `keld dev` outer-Job plus host-inner-Job tests prove developer-session cleanup
ordering only; they do not establish an installed updater launcher or a
MachineSeamless writer mechanism.

Windows recovery MUST NOT interpret `OpenJobObject` not-found as
`ActiveProcesses == 0`. A temporary object name is removed when its handle count
reaches zero even while kernel references may remain; a persisted Job name or attempt
ID is only a locator while an authenticated keeper still retains the exact handle.
Recovery requires a live retained-handle zero observation or separately qualified,
durable retirement evidence. Without either, it halts and preserves the journal and
all pointers. A supported user-mode boot-epoch proof has not been established.
Source: [Windows object life cycle](https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/life-cycle-of-an-object).

**Machine-UAC recovery-required state and recovery-only role (criterion 17; T4d).** In
`MachineUacDirect` only `keld-updater-helper.exe` writes, so an ordinary process never
recovers or repairs. When an ordinary startup takes the snapshot lease and finds either
a pending journal or no journal with an absent or undecodable `current` beside a valid
last-known-good, it infers and writes nothing: it assumes neither health nor
process-family retirement, commits and rolls back nothing, and preserves journal,
pointers and versions. It returns `UpdateError::Activation` with the new effect
`ActivationEffect::MachineRecoveryRequired(MachineRecoveryGuidance)` before admission or
app code. For `MachineUacDirect` that typed effect replaces both the
`UpdateError::Baseline` refusal of the machine-mode `current` repair, which carries no
activation effect (`crates/keld-update/src/windows_baseline/load.rs:255-262`), and the
`JournalBoundRecoveryRequired` that the journal-free selection returns for a pending
journal. A lease conflict stays `WriterActive` (criterion 20), and `MachineSeamlessDirect`
keeps its landed `UpdateError::Baseline` refusal until KEL-270 selects its authority.

`MachineRecoveryGuidance` is a closed set, each with one exact fix-guidance text that a
test pins byte for byte: `RecoveryDisabled` (the interim below: no supported resolution
other than administrator action), `RecoverNow` (an unlaunched `publish-pending` attempt
or an invalid `current`: run the recovery-only role, which needs no restart), and
`RestartFirst` (a launched attempt: restart Windows, then run the recovery-only role).

The effect's fix guidance names the recovery-only role, and the host's pre-admission
failure path offers it; neither needs an active selection or an ordinary host boot, and
an administrator can start the role directly. The role is the installed
`keld-updater-helper.exe` behind a fresh UAC prompt, launched and self-anchored as
"Helper launch and self-anchor" below requires. It reloads the protected
provenance of its installation, takes the exclusive writer lease, rereads and fully
revalidates provenance, floor, records, journal and both version trees, and then does
exactly one of the following:

1. With a launched journal phase, it resolves only through the KEL-53 phase rules,
   including their process-family proof, which after owner loss is "Machine-UAC
   owner-loss retirement" below.
2. With a `publish-pending` journal, which the lease alone proves unlaunched, it uses
   the abandon intent of owner decision D1 (refined): one intent argument of the single
   `next_activation_step` state machine (`activation.rs`), not a second state machine.
   Like every resumed owner it first re-mints the channel identities in a durable
   `PublishPending` record (creating no endpoint, because it launches nothing). The
   journal stays `PublishPending` through every step, so a crash at any step leaves a
   state that the next run resumes on the lease alone, without the logon-session proof.
   Under the abandon intent only, these landed mappings change (§5 lists them once):
   at the prior floor a published candidate retires (`RetireVersion` instead of
   `AdvanceFloor`, `activation.rs:203`) and a staged candidate is abandoned
   (`AbandonAttempt` instead of `PublishCandidate`, `:204`); at the candidate floor a
   published candidate retires (`RetireVersion` instead of `SelectCandidate`, `:210`)
   and an unpublished one is abandoned (`AbandonAttempt` instead of the
   `CandidateUnpublished` refusal, `:211-212`); `current` at the candidate restores the
   rollback target (`RestoreRollbackTarget` instead of `EnterAwaitingHealth`, `:215`);
   and `retirement_due`, which returns nothing for `PublishPending` today
   (`activation.rs:137-152`), names the candidate for `PublishPending` only under the
   abandon intent and only when no pointer names it.
   - If the floor is still at its prior value, it retires a published candidate and then
     removes the journal as `Abandoned`; a completed stage stays a never-selectable
     `incomplete-*` diagnostic. The floor never moved, so the same signed version stays
     retryable (criterion 14 and amendment F1).
   - If the floor is already at the candidate, it restores `current` to the rollback
     target if `current` names the candidate, retires a published candidate, then
     removes the journal. The version is consumed, as it would be on any route.
3. Without a journal, it owns the machine-mode repair of an invalid `current`: through
   the shared repair, it republishes last-known-good and reads it back, only when its own
   located version is last-known-good (the KEL-254 T2b located-version gate).

It never writes `AwaitingHealth` or `RollbackPending` for an attempt it did not launch,
and it creates no connect-back endpoint because it launches nothing.

It accepts no candidate, source, path or feed input and launches no application. A
declined prompt, failed revalidation or unproven process family writes nothing and
leaves the typed state in place; no manual filesystem work is required.

*Interim.* Until slice S10's rows pass, the recovery role is disabled: the helper
refuses it, right after a passing self-anchor, as for the activation role below, and
`MachineRecoveryRequired` carries `RecoveryDisabled`, whose guidance
says that no supported resolution exists other than administrator action, with journal,
pointers and versions preserved. Until slice S12's rows pass, a launched attempt still
carries `RecoveryDisabled`, because its owner-loss retirement proof is not admitted.
Until slice S11 lands, the activation role is disabled as well: right after a passing
self-anchor ("Helper launch and self-anchor"), before the lease and any write, the
helper refuses it with a typed error whose text is pinned (Coordination record, Linear
KEL-270 comment `7905ec8a-2529-4c23-90f4-878d315bc0e5`, 2026-10-07).

**Helper launch and self-anchor (criterion 17; T4d).** The host never launches the
helper from a path it was given or searched for. It derives the path only from
provenance admitted under the KEL-254 executable-located anchor and from the protected
records: for the activation role, its own selected version tree; for the recovery role,
the journal's rollback-target tree, else its candidate tree, whose
`keld-updater-helper.exe` has the journaled `helper_image_blake3`, or, with no journal,
the last-known-good tree. It opens that file beneath the pinned tree without following
reparse points, under KEL-254's file-identity and protection-profile checks, and offers
nothing when provenance is not authenticated. It starts that exact path with
`ShellExecuteExW` and the `runas` verb, for which UAC asks for consent or administrator
credentials
([SHELLEXECUTEINFOW](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ns-shellapi-shellexecuteinfow),
`ms.date` 2018-12-05), on a thread that first initializes COM as
`COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE`, because Microsoft says COM should be
initialized before `ShellExecuteEx`
([ShellExecuteExW](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shellexecuteexw),
`ms.date` 2018-12-05). That thread is a dedicated launch thread: it enters the
single-threaded apartment with `CoInitializeEx`, makes the one call and leaves with
`CoUninitialize` before it ends. The call sets `SEE_MASK_NOCLOSEPROCESS`;
`SEE_MASK_NOASYNC`, which Microsoft requires when the calling thread has no message
loop or ends soon after the call; and `SEE_MASK_FLAG_NO_UI`, which suppresses error
dialogs while security prompts such as UAC's still show. Its `lpDirectory` is the
helper's own directory, never the host's current directory, which a null value would
pass on; its `nShow` is `SW_HIDE`; and it names no owner window
([SHELLEXECUTEINFOW](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ns-shellapi-shellexecuteinfow),
`ms.date` 2018-12-05; KEL-270 T4d PR-0 review, 2026-10-07, recording the S9b launch).
It passes exactly one argument, which conveys no authority: the
bootstrap rendezvous name for the activation role ("Machine-UAC bootstrap"), or the
fixed recovery-role selector `--recovery-role`, exact ASCII and case-sensitive, defined
once in `keld-runtime` and imported by the helper's argument check (Coordination record,
Linear KEL-270 comment `7905ec8a-2529-4c23-90f4-878d315bc0e5`, 2026-10-07). An
administrator may start the same file directly.

Loader hardening covers what runs before and after `main` separately. Before `main`,
`/DEPENDENTLOADFLAG:0x800` makes the operating system resolve the module's own static
imports only from System32; it covers only those imports
([/DEPENDENTLOADFLAG](https://learn.microsoft.com/en-us/cpp/build/reference/dependentloadflag),
`ms.date` 2020-01-22). The helper therefore also links the C runtime statically, through
its build script's static-runtime and `/NODEFAULTLIB` link arguments for this binary
only, because `+crt-static` is a target-wide flag, so no runtime DLL is searched for;
and a build-time check pins its import table to a fixed allowlist of System32 DLLs. As
its first statement, `main` calls `SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32)`,
which governs every later load
([SetDefaultDllDirectories](https://learn.microsoft.com/en-us/windows/win32/api/libloaderapi/nf-libloaderapi-setdefaultdlldirectories),
`ms.date` 2018-12-05). An elevated process must not search the current directory or any
other location an ordinary user can write. The helper's edge set is enforced
mechanically: a `cargo tree` and `cargo-deny` check refuses `keld-core`, `keld-wv`,
`keld-host`, `keld-cli`, `keld-native` and any WebView binding crate in its normal
dependency closure, and a `clippy` `disallowed-methods` list in the helper crate names
every `keld-runtime` entry point that starts Bun or an app role, so no reachable code
path or import starts Bun or a WebView.

The host's derivation decides which file runs; the helper's self-anchor covers in-tree
tampering. The helper locates its installation from its own image by KEL-254 §4's
executable-located rule, with its literal file name `keld-updater-helper.exe` in place
of `keld-host.exe`; the one locator function takes a closed choice of these two images,
never a free-form name. Like the host, the helper carries its own `ExpectedAppIdentity`
payload, which KEL-19's writer embeds before the helper's final Authenticode signature,
and reads it with `ExpectedAppIdentity::from_signed_image` (KEL-254 T3 Part B) on its own
handle verified by the single `keld-guard` Authenticode owner (owner decision D4). That
expectation, the located roots' file identities and the recorded mode's protection
profile anchor the record, so the record never anchors itself. Before it takes the lease
or writes, the helper refuses every role unless the recorded mode is `MachineUacDirect`,
its Authenticode signer equals the record's publisher scope, and its own image is the
expected one: with a journal, its digest equals `helper_image_blake3`; without one, its
located version is last-known-good for the recovery role and the selected current
version for the activation role.

*Running-image binding.* (KEL-270 owner decision `740998f4-9a47-4527-9e1b-1adb10f4836e`,
2026-10-07, item 1, on the S9c review finding in Linear KEL-270 comment `c4921888`.)
Today the helper and the installed host each verify the file that their `current_exe()`
path opens (`crates/keld-updater-helper/src/helper.rs:70-93`;
`crates/keld-core/src/app_session.rs:2360-2371`, `:2475-2497`), and the verifier refuses
only a leaf reparse point (`crates/keld-guard/src/windows_authenticode.rs:207-235`), so
a junction earlier in the path is followed. After the open the binding holds: the
located tree's file must be the verified handle's file
(`crates/keld-update/src/windows_baseline/locate.rs:265-281`). Before the open there may
be a gap, inferred and not demonstrated: a different genuine image of the same publisher,
started through a user-owned junction that is then retargeted at a protected version
tree, would leave the in-tree file verified while other code runs. The single
`keld-guard` Authenticode owner therefore binds the file that it opens and verifies to
the running image: it compares that file object with the running process's NT image
path, from `QueryFullProcessImageNameW` with `PROCESS_NAME_NATIVE` or from
`GetMappedFileNameW` on the module base, and refuses a mismatch before it returns the
verified image. The host and the helper verify their own image only through this
binding; verifying another image, as the tests that check a built host do
(`crates/keld-pack/tests/real_host_acceptance/coverage.rs:123`), keeps the path-only
open. It covers both images, the KEL-254 installed host and this helper, and lands as
slice S9d, before S10 and S11 add the helper's write path. A research spike in the
nested research checkout comes first and answers whether `GetModuleFileNameW` and
`current_exe()` keep or resolve a non-leaf junction, and whether a junction can be
retargeted while an image beneath it is mapped; S9d fixes from its receipts which of the
two sources it reads and how it compares the file object with that path. Until S9d
lands, the possible gap stays open; it is not exploitable through the helper today,
because both roles refuse after the anchor (*Interim*), no helper write path exists and
an elevated launch needs administrator consent (comment `c4921888`). Whether the
installed host's boot is exposed before S9d lands is unknown; S9d's receipts decide it.

**Machine-UAC owner-loss retirement (PR #374 review, 2026-10-05: the owner chose a
durable proof over a permanent typed halt or pointer-only rollback and delegated the
mechanism; exact-content approval pending; qualified in T4d).** For `MachineUacDirect`,
a launched attempt (`awaiting-health`, `health-accepted` or `rollback-pending`) whose
owner was lost resolves only when two independent facts hold. An unlaunched
`publish-pending` attempt keeps the lease-only rule below.

1. *No owner can still write.* The recovery-only helper holds the exclusive writer
   lease. Every live transaction owner (the UAC helper or a keeper) retains the
   share-zero lease, and a terminated process's handles are closed
   ([Terminating a Process](https://learn.microsoft.com/en-us/windows/win32/procthread/terminating-a-process),
   `ms.date` 2025-07-14), so acquiring the lease excludes them (the `publish-pending`
   rule below). No PID, Job-name or process-absence observation is used.
2. *The candidate family has exited.* Process termination is asynchronous and waits for
   pending I/O
   ([TerminateProcess](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-terminateprocess),
   `ms.date` 2025-12-30), so closing the last handle of a kill-on-close Job only starts
   the family's termination. The journal records the initiating user's logon session
   (`AuthenticationId` and its logon time) durably before launch; a zero logon time or,
   by owner decision 2026-10-06 (zero LUID refused), a zero `AuthenticationId` refuses
   before `PublishPending` ("Machine-UAC bootstrap" item 6), because the session query
   below would trivially report no such logon session for a LUID that no live session
   can hold. Every family process must run with a primary token that references
   that session (the census below). Microsoft documents, for the authentication-package
   callback, that a logon session terminates when the last token referencing it is
   deleted
   ([LSA_AP_LOGON_TERMINATED](https://learn.microsoft.com/en-us/windows/win32/api/ntsecpkg/nc-ntsecpkg-lsa_ap_logon_terminated),
   `ms.date` 2018-12-05;
   [SeRegisterLogonSessionTerminatedRoutine](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/nf-ntifs-seregisterlogonsessionterminatedroutine),
   `ms.date` 2018-04-16); that the session query below therefore proves the family's
   tokens are gone is an inference that T4d must observe, not a documented guarantee.
   Recovery runs as an administrator, which may read any logon session
   ([LsaGetLogonSessionData](https://learn.microsoft.com/en-us/windows/win32/api/ntsecapi/nf-ntsecapi-lsagetlogonsessiondata),
   `ms.date` 2018-12-05). It treats the family as exited only when that query reports
   that no such logon session exists (`STATUS_NO_SUCH_LOGON_SESSION`, the one accepted
   status; the name is inferred and T4d confirms it on real Windows), or when the
   session now holding that ID has a different logon time,
   because a locally unique ID is unique only until restart
   ([AllocateLocallyUniqueId](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-allocatelocallyuniqueid),
   `ms.date` 2018-12-05). A live session, access denial or any other status halts.

Fact 2 holds only if every process that runs an image from the candidate version tree,
or runs code or a command that the family supplies (for example a COM server or a
scheduled task registered to a candidate-tree image), runs inside the attempt Job under
the initiating logon session. System brokers that load no candidate image are out of
scope. T4d proves this by census and static scan, not by inference from the launch
route or by audit. The census: at Ready, at health acceptance and immediately before the
helper clears the Job limit, every process in the attempt Job, which the helper lists
through the `keld-runtime` process-ID lister that slice S6b3 lands and S12 reuses
(§5), and every process whose image
lies in the candidate version tree, including the WebView2 browser process and its
children and each admitted LPAC role, is a member of the attempt Job, and its primary
token reports the journaled `AuthenticationId` through
`GetTokenInformation(TokenStatistics)`, the LUID of the logon session that a token
represents, which many tokens may share
([TOKEN_STATISTICS](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-token_statistics),
`ms.date` 2018-12-05). The owner also checks the candidate host's token when it accepts
the claim ("Candidate connect-back"). The static scan: the host and WebView crates
contain no `CreateProcessWithLogonW`, no `runas` verb, no `ITaskService` use and no
local-server COM activation (`CLSCTX_LOCAL_SERVER`). Until the census and the scan pass,
fact 2 is not admitted. A reboot is not itself evidence; the session query after it is.
That a full restart, a sign-out or a Fast Startup shutdown ends the initiating session
is a T4d qualification target, not an assumed fact: Microsoft documents only that a
locally unique ID is unique until restart, which the logon-time comparison covers, and
any remaining reference to a token of that session, such as one a service holds, keeps
it alive, so recovery then halts. A helper crash without a restart therefore leaves
`MachineRecoveryRequired` with `RestartFirst` guidance, because a restart is the
expected way to end the session; the query decides. The same proof covers
`health-accepted`, where the healthy application legitimately outlives the helper,
because the landed `WindowsRecoveryInspection::recover` requires an exact
process-family retirement binding for every phase
(`crates/keld-update/src/windows_baseline/activate.rs:603-627`).

Kill-on-close is kept only for prompt termination. The UAC helper, which launches the
candidate, alone holds the attempt's unnamed `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` Job,
without `JOB_OBJECT_LIMIT_BREAKAWAY_OK` or `JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK`, and
makes the candidate a member before its first instruction runs. Helper death therefore
ends the candidate during the health window, a deliberate crash-ownership change, and
the helper clears the limit once `health-accepted` is durable, through the one
`keld-runtime` clear primitive, `WindowsProcessJob::release_family`, that slice S6b3
lands for `PerUserDirect` ("Candidate release after commit"); Machine-UAC shares only
that primitive and keeps this timing. Whether the elevated helper, or the candidate it
launches with `CreateProcessWithTokenW`, also joins the initiating host's host-death Job
is unknown: no evidence in this specification covers the Job membership of either
launch, and a launch that names a parent process inherits that parent's job object
([UpdateProcThreadAttribute](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute),
`PROC_THREAD_ATTRIBUTE_PARENT_PROCESS`, `ms.date` 2021-02-02). S11's §7
rows record both memberships and whether the candidate survives the initiating host's
exit ("17 (Machine-UAC Job inheritance)"); a candidate inside that host-death Job stops
S11 for an owner decision, because the defect that "Candidate release after commit"
removes for `PerUserDirect` would recur here, and the elevated helper cannot classify
the host's family. This amendment specifies no probe. Job-name absence, PID
enumeration and the death of Job-handle holders still prove nothing about the family,
and the seamless keeper slice's rule below is unchanged. The helper reuses the existing
Windows Job wrappers; the new FFI calls, including the `keld-guard` logon-session
wrapper, and their owners are listed once in §5.
The two logon-session fields are a journal schema revision under
the wire review gate. Rejected alternatives: Job-handle-holder death as the family proof
(it proves only that termination started); a separate PID and creation-time check for
the coordinator as retirement evidence (the lease already excludes every live owner; the
journaled owner process ID and creation time serve only the claimant's server check in
"Candidate connect-back"); a keeper that retains the
Job handle (it defeats kill-on-close and enlarges the privileged surface only to avoid a
restart); a permanent typed halt; and pointer-only rollback without retirement proof.
Until the T4d rows pass, neither fact is admitted and the case halts in
`MachineRecoveryRequired`.

**Bounded per-attempt lifecycle keeper (proof slice; no production activation writes).**
The first Windows implementation slice MUST use the exact unnamed attempt Job object
and transfer/duplicate its handle; opening a Job by persisted name is forbidden. The
keeper runs outside that Job and receives only the least rights needed to terminate
the exact family and query its active count (`JOB_OBJECT_TERMINATE` and
`JOB_OBJECT_QUERY`); it receives no assign, limit-change, file-write or transaction
command authority. It retains only the reduced, non-writable duplicate of the stable
`activation.lock` object needed to keep the existing writer exclusion alive, plus the
exact coordinator process observation needed to distinguish coordinator death from
pipe loss. KEL-53 remains the only writer of journal, floor and pointers.

The keeper may terminate and observe the exact Job only after the authenticated
coordinator process object is signaled. Writer exclusion is not retired when a pipe
closes, a Job name disappears, or a PID is absent. Retirement requires a fresh,
attempt-bound, one-shot authenticated handoff of the exact Job witness. The selected
successor independently queries that witness and acknowledges the exact zero result
over the authenticated connection. The keeper releases its lock-retention handle only
after that acknowledgement; the successor then acquires the exclusive writer lease and
revalidates the protected install and journal attempt. Reacquiring before release is
impossible while the share-zero retention handle remains open. A competing writer that
wins after release cannot mutate a pending attempt without the matching retirement
witness and MUST refuse recovery. No authenticated handoff or proof means journal and
all pointers remain unchanged and recovery halts.

Adversarial proof MUST separately cover coordinator death, keeper death, both owners
lost before handoff, wrong Job/host, stale and replayed attempts, competing writer,
exact Job-family zero, and fake endpoint. The lifecycle code must not be connected to
production pointer/floor/activation/commit writes until these controls pass. Reboot,
hibernate and all-owners-lost recovery remain unsupported unless a documented Windows
mechanism proves durable retirement; otherwise ambiguous recovery halts with evidence
preserved. (Machine-UAC owner loss uses its own two-fact rule above; this slice's rule
is unchanged.)

The single-writer transition is:

1. verify protected direct provenance and mode, acquire the mode-supplied stable lease,
   then verify/extract `full`, including `.complete` and policy;
2. retain and validate current as the rollback target plus both known-good slots;
3. persist `PublishPending`;
4. rename the completed stage to the candidate's absent version name, then fully
   re-verify and pin the published candidate;
5. advance the semantic-version trust floor;
6. publish `current` to the candidate;
7. persist `AwaitingHealth` and launch with a private health channel;
8. on exact health, persist `HealthAccepted`, publish the prior LKG to
   `previous-known-good`, publish `last-known-good` to candidate, retire the
   superseded older version (if any), then remove the journal; deleting retired trees is
   best-effort cleanup that never touches either known-good slot;
9. on failure, persist `RollbackPending`, publish `current` to the
   validated rollback target, retire the failed candidate, remove the journal, then
   report failure.

Retirement is the only mutation of a published version. While the journal still
authorizes it, the writer renames the one unreferenced version directory, with the
same-parent absent-target write-through adapter, to a generated `retired-<64 hex>`
name. Generated `incomplete-*` and `retired-*` names can never equal a SemVer version
directory, so the census admits them only as never-selectable diagnostics. Retiring
under the journal closes the crash window in which a removed journal would leave an
orphan complete version that halts every later writer. On NTFS an open file handle
anywhere in the retiring tree, with any sharing mode, makes the rename fail; the journal
then stays for journal-bound recovery. A process still executing a mapped image from
that tree does not block the rename: it keeps running from the retired tree, cannot
open further files by their original path, and its tree is deleted only after it
exits. A launched candidate is therefore retired only after process-family retirement,
and the host must not run other instances from a superseded version.
Journal removal is a write-through rename of the journal to a generated `pending-*`
leaf followed by deletion. A crash before any record sibling's publication rename
leaves only such a `pending-*` file. The census admits `pending-*` names; before its
first write, the next transaction removes each one only after verifying a regular,
single-link file with the exact installation profile, and refuses anything else.
A new attempt never publishes a version before its `PublishPending` journal is
durable. A refused start, or a process crash before the journal, leaves only the
completed stage under its `incomplete-*` name: no version, record or journal is
published, and the next resolution deletes stale completed `incomplete-*` stages
together with `retired-*` trees through retained handles. After installation only the
writer-lease holder completes a stage (the initializer does so only in an empty
`versions` before provenance exists), so a stage without a completion record, which may
be a live or failed extraction by a root without the lease, stays as a diagnostic. Recovery identifies the stage by its
completion record, which must name the exact journaled candidate; a pending attempt
whose candidate is neither published nor staged at the recorded prior floor is
abandoned with no record changed. Only a missing completion record or one naming
another artifact excludes a stage; any fault reading a stage halts recovery with the
journal intact. A published copy that fails its full re-verification is retired under
the journal and the next exact stage is tried; with none left the attempt is abandoned
before the floor moves, so the same signed version may be retried. A census fault about
other entries retires nothing and keeps the journal. An installation that already holds an
orphan complete version from the earlier publication order still halts the ordinary
loader; only the explicit unjournaled-version repair, admitted when no journal exists
and every record validates, retires it under the writer lease. The repair first verifies and pins every
referenced version, removes stale `pending-*` record siblings, and admits for retirement
only strict-SemVer entries whose completion record names that version in the
installation's scope; any other unknown or damaged entry, including a non-directory under a
generated name, refuses the repair before any rename and needs manual recovery.

A `PublishPending` journal is resumable under the exclusive writer lease alone: no
candidate is launched before `AwaitingHealth` is durable, and every live transaction
owner retains the share-zero lease (or its keeper retains a duplicate), so acquiring
the lease proves no prior owner can still write and no candidate family exists. Every
resumed owner durably re-mints the attempt's health and lifecycle channel identities
before continuing, so no health receipt or retirement witness from a lost owner binds
to the resumed run; an owner that will launch holds its new connect-back endpoint
before that record ("Candidate connect-back"). Launched phases still require an exact
process-family retirement binding. That binding is an exact
installation/attempt/lifecycle-channel value that the lease-holding attempt owner
composes from the QF1 retirement witness or its own retained Job-zero observation, or
that the Machine-UAC recovery-only helper composes from the owner-loss retirement facts
above once T4d qualifies them; `keld-update` cannot authenticate its producer.
A sealed witness type was rejected
because it would add a `keld-update` -> `keld-runtime` edge outside the approved crate
graph.

`HealthAccepted` records `BLAKE3(UTF8("keld.activation-health-receipt/v1\0") ||
attempt_id || health_channel_id || u64_le(n) || a)`, where `a` is the canonical
artifact-identity encoding of the candidate and `n` its byte length. The receipt binds
the attempt, its private health channel and the exact candidate; recovery recomputes
the digest from the journal's own fields and halts on a mismatch.

Windows replaces fixed mutable record slots through a narrow same-parent
`MoveFileExW(MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)` adapter; the existing
absent-target version-directory publication remains separate. A transition writes a
new protected sibling with the mode's exact DACL, flushes and reads it back, closes
conflicting record pins, performs the fixed replacement under the stable lease, then
reopens/reads back the exact bytes and descriptor before the next step. It does not
truncate in place, copy across volumes, schedule work after reboot, or accept caller
paths/flags. A replacement error is effect-aware; it does not claim the old bytes stayed
unchanged after a possible publication.

Startup without a journal validates current, both known-good slots, their complete
markers/policies and the floor. A valid current must equal last-known-good or
previous-known-good; any other complete artifact is an orphan and halts (generated
`incomplete-*` and `retired-*` diagnostics are not artifacts). If current is
invalid but last-known-good is valid, recovery republishes last-known-good. A
missing/invalid last-known-good after installation halts even when current runs;
previous-known-good may be absent only before the first successful update. Recovery
acquires the attempt lease and proves the prior
coordinator/candidate family exited. Valid `PublishPending` with current still at
the rollback target accepts only the recorded prior floor or exact candidate floor
before resuming; at the prior floor it first publishes a still-staged candidate and
abandons an attempt whose candidate is neither published nor staged. With current
already at the candidate it requires floor exactly equal
to candidate and advances to `AwaitingHealth` without republishing. Every other
combination, including floor above candidate, halts. `AwaitingHealth` rolls back
only after the process-family
proof. `HealthAccepted` finishes both known-good publications and the superseded
version's retirement. `RollbackPending` finishes rollback and the candidate's
retirement only after floor, both known-good slots,
coordinator/helper identity, optional health identity and current exactly match its
recorded context. Corrupt or mixed state halts without deleting evidence.

**Candidate connect-back (criterion 8; every direct mode).** This paragraph is the
single owner of the connect-back endpoint, the claim, its acceptance and refusal, and the
`keld-attempt` message set. The launched candidate inherits no attempt endpoint and
takes no authority from argv or environment. The component that launches the candidate
is the attempt owner: the `PerUserDirect` host coordinator or criterion-10 post-exit
helper, or the Machine-UAC `keld-updater-helper.exe`. It holds the exclusive writer
lease, and `keld-update` mints the attempt, health and lifecycle identities inside it.
One locator function derives the endpoint name from the provenance-derived installation
ID and the minted attempt and health-channel IDs, in the dedicated
`\\.\pipe\keld-attempt-<64 lowercase hex>` namespace; the D2 bootstrap name uses the same
function with its own purpose and nonce. The name conveys no authority, and the
keeper's rendezvous locator is never a health endpoint.

*Locator.* (approved: KEL-270 owner decision `eff8e2fb`, 2026-10-06)
The name is `\\.\pipe\keld-attempt-` followed by the 32-byte BLAKE3 digest of
`UTF8("keld.attempt-endpoint/v1\0") || purpose || installation_id || a || b`, rendered
as 64 lowercase hexadecimal digits, each byte in digest order with its high nibble first
(the landed lifecycle rendering, `bootstrap.rs:2104-2113`). `purpose` is one byte and
every other input is exactly 32 bytes, so no input needs a length prefix. Purpose `1`
(connect-back) takes `a` = attempt ID and `b` = health-channel ID. Purpose `2`
(bootstrap) takes `a` = the bootstrap nonce and `b` = 32 zero bytes. The attempt
purposes are a separate closed type in `attempt.rs`; `WindowsLifecyclePurpose` is never
reused for them (criterion 17). The function refuses an all-zero installation ID,
attempt ID, health-channel ID or nonce and any two equal inputs other than that zero
`b`, as the landed lifecycle binding does (`bootstrap.rs:829-846`). `keld-ipc` alone
owns the whole function (prefix, domain, purpose values, input order, hash, rendering
and refusals), together with its `is_attempt_endpoint` predicate, which shares the
function's one crate-private prefix constant, and the codec whose `BH1` purpose byte
uses the same purpose values. It hashes only what its callers pass: `keld-update` keeps
the installation-ID derivation and the sole minting of the attempt and health-channel
IDs. The attempt server derives its name from the same IDs that its `AC1` carries, and
the attempt client performs the locator check before it sends `AA1`, so no caller can
skip it. The crates' `Cargo.toml` edges limit the choice. `keld-update` and `keld-ipc`
have no normal edge in either direction; §5 keeps `keld-update`'s edges to `keld-ipc`
dev-only, and a `keld-ipc -> keld-update` edge would pull the updater into every
`keld-ipc` consumer. A digest in `keld-update` would split the locator rule across two
crates, as the landed lifecycle renderer does, and make the check before `AA1` a
callback supplied by the caller; one owner keeps the rule in one crate and the check
inside the client. `keld-core` cannot own it, because the helper's edge set excludes
`keld-core` ("Helper launch and self-anchor"), and `keld-runtime` and `keld-pack` own no
pipe namespace. Every caller already links `keld-ipc`: the host through `keld-core` and
`keld-runtime`, and the helper through its §5 edge. The cost is one Windows-only
`keld-ipc` edge to the workspace-pinned `blake3` (`=1.8.7`), which `keld-update` and
`keld-pack` already lock (§8). The locator runs once per endpoint or claim, off the kipc
hot path. Golden vectors, which the workspace-pinned `blake3` crate and an independent
implementation reproduce: with installation ID `11`×32, attempt ID `22`×32 and
health-channel ID `33`×32, purpose `1` gives
`a56a565b56c571bd19b06b8e62845fa5a14c28fecd5611c94e4c90e8a1641ba3`; with installation ID
`11`×32 and bootstrap nonce `44`×32, purpose `2` gives
`879bfdd74649e498f349aafd7a7661d46bceddc4f2ddd0e6a78edde68cb3f555`; exchanging `a` and
`b` in the first vector changes the digest. S4 lands purpose `1` with its vector;
purpose `2`, its zero `b` and its vector land with S11, the first slice that derives a
bootstrap name (§6).

*Order.* `keld-update` first mints the identities without writing anything, through the
mint-then-journal seam that splits `WindowsExtractionRoot::begin_activation` and
`resume_unlaunched` (§8). Those two entry points and `recover` take the attempt owner's
`keld_guard::VerifiedWindowsImage` from its one Authenticode verification, never a
bare file or a digest: `keld-update` derives the journaled `helper_image_blake3` from
that image's file (`VerifiedWindowsImage::file`) with its single image-digest owner
(`crates/keld-update/src/windows_baseline.rs:676`), which the helper's self-anchor and the
claimant's candidate-boot read already use, so no caller computes or supplies the digest
(KEL-270 owner decision `740998f4-9a47-4527-9e1b-1adb10f4836e`, 2026-10-07, item 3;
slice S6b2). Once slice S9d lands, that verification also binds the image's file to the
running image ("Helper launch and self-anchor", *Running-image binding*). The owner then
creates and holds the endpoint and reads its
descriptor back, and only then is the durable record written that first reveals the
name, with the owner's `owner_process_id` and `owner_creation_time`: `PublishPending` for
a fresh attempt, or the re-mint record for a resumed owner. The endpoint is therefore
held before anyone can derive its name from the journal, which BUILTIN Users may read.
If the name already exists, or creation or readback fails, a fresh attempt refuses with
`ProtectedStateUnchanged` before any protected write, and a resumed owner, which only
`PerUserDirect` has, refuses with the journal unchanged and the effect
`JournalBoundRecoveryRequired` that the landed `Transaction::fault` gives every
journaled refusal (`windows_baseline/activate.rs:1047-1057`). A launch refusal after
`AwaitingHealth` is durable rolls the attempt back with `CandidateLaunch` under the same
lease, composing the retirement binding from the owner's own retained Job-zero
observation; "Machine-UAC bootstrap" item 6 owns which refusals come before
`PublishPending` instead, and why a crash inside that rollback needs the owner-loss
proof. The recovery-only role launches nothing and creates no endpoint.

*Creation.* The owner creates the endpoint through the landed `keld-ipc` named-pipe
server: at most one instance, `FILE_FLAG_FIRST_PIPE_INSTANCE`,
`PIPE_REJECT_REMOTE_CLIENTS`, a non-inheritable handle and a protected DACL that it reads
back. The single ACE grants the initiating user SID, not the owner's own TokenUser, only
the landed `keld-ipc` access mask `0x0012019B`: the individual read/write data, attribute
and extended-attribute rights, `READ_CONTROL` so the claimant can read the descriptor
back, and `SYNCHRONIZE`; never `FILE_CREATE_PIPE_INSTANCE`, which `FILE_GENERIC_WRITE`
would include, and never `WRITE_DAC` or `WRITE_OWNER`. Every connect-back endpoint that
the elevated helper creates carries the explicit owner `O:BA` (owner decision D5); a
`PerUserDirect` endpoint's owner is the user, and the host-created bootstrap endpoint's
owner is the host's own user SID ("Machine-UAC bootstrap"). The descriptor also carries an explicit Medium mandatory label with
`SYSTEM_MANDATORY_LABEL_NO_WRITE_UP`, which denies Low-integrity and AppContainer
writers. Microsoft documents that the objects a process creates receive its integrity
level and that unlabelled objects are treated as Medium
([Mandatory Integrity Control](https://learn.microsoft.com/en-us/windows/win32/secauthz/mandatory-integrity-control),
`ms.date` 2025-07-08); whether an elevated helper's unlabelled pipe would be High, and
so deny the Medium candidate, is unverified, so the label stays explicit and T4d
observes the default. `FILE_FLAG_FIRST_PIPE_INSTANCE` fails only the flag-setter's own
call when the name already exists; any other creator of the name is excluded by the
maximum of one instance and by the missing `FILE_CREATE_PIPE_INSTANCE` grant
([CreateNamedPipeW](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-createnamedpipew),
`ms.date` 2022-08-05;
[Named Pipe Security and Access Rights](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights),
`ms.date` 2018-05-31). `PIPE_REJECT_REMOTE_CLIENTS` is load-bearing: a client that opens
a pipe through the local SMB server can spoof the client process ID that the named-pipe
file system reports
([Windows Exploitation Tricks: Spoofing Named Pipe Client PID](https://googleprojectzero.blogspot.com/2019/09/windows-exploitation-tricks-spoofing.html),
Project Zero, September 2019); that the flag rejects such a loopback open is a T4d
qualification target.

*Claim.* The owner starts the candidate with the endpoint name as its single rendezvous
argument, which carries no authority (owner decision D3). Only a process started with
that argument attempts a claim, and it accepts the argument only in the exact local
shape of "Machine-UAC bootstrap" item 3, refusing anything else before any open. It takes no snapshot lease and never reads the journal
or any other mutable record before acceptance. To learn its install mode and
installation ID it reads only its immutable protected provenance record, which no writer
ever replaces, through the executable-located anchor of KEL-254 §4. A future mode
transition or v1 migration that replaces provenance (criterion 1) must account for these
claimants, which hold no lease. It opens the
name with `SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION`, so a server that is not the
owner can at most identify the claimant, never impersonate it; named-pipe servers
otherwise receive impersonation by default
([CreateFileW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew),
`ms.date` 2022-08-16;
[Impersonation Levels](https://learn.microsoft.com/en-us/windows/win32/secauthz/impersonation-levels),
`ms.date` 2018-05-31). Before it sends anything it requires: `GetNamedPipeServerSessionId`
equal to its own session; the descriptor exactly the protected form above for its own
TokenUser, owned by BUILTIN Administrators in `MachineUacDirect` or by its own user SID
in `PerUserDirect`; and, when it can open the process that `GetNamedPipeServerProcessId`
names with `PROCESS_QUERY_LIMITED_INFORMATION`, that process's image is the owner image
that its install mode admits (`keld-updater-helper.exe` in `MachineUacDirect`; the
`keld-host.exe` or the post-exit helper in `PerUserDirect`) in a version tree of its own
installation, by file identity. Whether a Medium process may open an elevated owner,
including one approved with alternate administrator credentials, is a qualification
target; when it cannot, the `O:BA` owner, the session and, after acceptance, the
server's process ID bind the owner (owner decision D5), because a Medium squatter cannot
make BUILTIN Administrators the owner. The connected handle stays non-inheritable and
is never placed in any role's handle list.

*Acceptance.* For each connection, the owner reads the client process ID with
`GetNamedPipeClientProcessId` immediately after connect and under a per-connection
deadline, so a silent connector cannot hold the single instance until the health
deadline. That ID names only the process that opened the pipe, not necessarily the
writer, so it is a locator: the owner opens that process and requires three facts.
First, `CompareObjectHandles` reports that the opened handle and the retained launch
handle refer to the same kernel object
([CompareObjectHandles](https://learn.microsoft.com/en-us/windows/win32/api/handleapi/nf-handleapi-compareobjecthandles),
`ms.date` 2018-12-05); a process ID by itself identifies a process only until that
process terminates
([Process Handles and Identifiers](https://learn.microsoft.com/en-us/windows/win32/procthread/process-handles-and-identifiers),
`ms.date` 2025-07-14). Second, the retained launch handle is still unsignaled. Third, its
process ID and creation time (`GetProcessTimes`) equal those the owner recorded at
launch. The owner then impersonates the claim's writer, at the identification level the
claimant granted, only to query its token, and requires the initiating TokenUser, the
initiating `AuthenticationId` (`TokenStatistics`), the initiating session, Medium
integrity and a non-elevated token. It reverts before anything else, and a failed
`RevertToSelf` terminates the owner. The claimant's `AH1` carries only what it knows
without the journal: the installation ID, its nonce and its process ID. The owner's
`AC1` supplies the attempt and health-channel IDs. Before it sends `AA1`, the claimant
requires the locator function over the installation, attempt and health-channel IDs to
yield exactly its own rendezvous name, so the `AH1`, `AC1` and `AA1` exchange binds all
three IDs, both fresh nonces and both process IDs to the name it was launched with.

*Refusal.* Any mismatch on either side refuses that claimant, and the refused process
refuses its own start with a typed `WriterActive` before app code; this paragraph owns
that result. The owner disconnects a refused claimant with `DisconnectNamedPipe` and
re-arms the same pipe instance for the next client, which is the landed `keld-ipc`
`disconnect_for_retry` behavior; the instance's one-shot is consumed only by the
claimant the owner accepts. Refusals never extend the health deadline, so a process that
keeps connecting can at most cause a health timeout and the ordinary rollback. A second
instance or a same-user Medium copy of the candidate image is refused by
`CompareObjectHandles`; an LPAC copy that a hostile role starts cannot open the endpoint
at all, because the DACL and the label deny it.

*After acceptance.* Only the owner's acceptance selects candidate boot mode before
ordinary updater startup. The accepted candidate performs criterion 20's authenticated
candidate-boot read while the owner replaces no mutable record. It requires an
`AwaitingHealth` journal whose candidate is the version tree holding its running
executable, whose attempt and health-channel IDs equal the `AC1` values that it already
matched against its rendezvous name, and whose `owner_process_id` equals the
server's process ID and, when the server process could be opened, whose
`owner_creation_time` and `helper_image_blake3` match that process. A mismatch refuses
with a typed `WriterActive` before app code. It then validates the exact
attempt/current/artifact, skips the writer lock and orphan recovery, closes its
mutable-record pins, acknowledges bootstrap, starts the app and reports boot, Ready and
health to the owner. It cannot write the journal or commit itself. With no live owner,
startup follows the recovery path above.

*Messages.* The `keld-attempt` subprotocol has its own magic values and one owning
module, `crates/keld-ipc/src/attempt.rs` (codec, server and client, following the
lifecycle records in `bootstrap.rs`), with the Windows pipe primitives in
`windows_named_pipe.rs`. Its closed message set is:
- bootstrap (owner decision D2): the helper's `KELD-BH1` hello (purpose, the
  installation ID from its self-anchor, client nonce, client process ID); the host's
  `KELD-BQ1` request (the hello's binding plus server nonce, server process ID and the
  bounded source locator); the helper's `KELD-BA1` acknowledgement of that transcript;
  the host's `KELD-BR1` receipt, sent after it consumes its one-shot; and the helper's
  `KELD-BO1` outcome, a closed outcome class, before it closes;
- claim: the candidate's `KELD-AH1` claim (installation ID, client nonce, client
  process ID); the owner's `KELD-AC1` challenge (the attempt and health-channel IDs, the
  server nonce and the server process ID); the candidate's `KELD-AA1` acknowledgement of
  the whole transcript, sent only after its locator check; and the owner's `KELD-AR1`
  acceptance receipt, sent only after it consumes its one-shot;
- health: the candidate's `KELD-AB1` boot acknowledgement after criterion 20's read
  (attempt, health channel, artifact), `KELD-AY1` Ready and `KELD-AF1` failure (a closed
  class: bootstrap read refused, application exit before Ready, or boot error), and the
  owner's `KELD-AK1` health result (accepted or rolled back).

(approved: KEL-270 owner decision `eff8e2fb`, 2026-10-06) The records
follow the lifecycle records' fixed-size, little-endian style, and the table fixes
their bytes (*Proposals*). Offsets are `start..end` byte ranges. As in the `LC1` record
above, a magic is its 8 ASCII bytes and a PID is a little-endian `u32`. An ID, nonce or
digest is a raw 32-byte value, matching the 32-byte IDs of the journal and of the
installation binding above. A purpose, class or result byte is a closed set, like the
`LC1` purpose and the closed classes of the lists above, and any other value refuses.
Each record carries exactly the fields that its bootstrap, claim or health list above
names, and a receipt carries the transcript that it confirms (*Transcript*). Fields keep
the order of the `LC1` record (purpose, installation ID, attempt ID, channel ID, client
nonce, server nonce, client PID, server PID; `bootstrap.rs:1471-1504`), and a field that
`LC1` lacks comes last.

| Record (sender) | Bytes | Layout |
|---|---|---|
| `KELD-AH1` (candidate) | 76 | `0..8` magic, `8..40` installation ID, `40..72` client nonce, `72..76` client PID |
| `KELD-AC1` (owner) | 108 | `0..8` magic, `8..40` attempt ID, `40..72` health-channel ID, `72..104` server nonce, `104..108` server PID |
| `KELD-AA1` (candidate), `KELD-AR1` (owner) | 176 | `0..8` magic, `8..40` installation ID, `40..72` attempt ID, `72..104` health-channel ID, `104..136` client nonce, `136..168` server nonce, `168..172` client PID, `172..176` server PID |
| `KELD-AB1` (candidate) | 104 | `0..8` magic, `8..40` attempt ID, `40..72` health-channel ID, `72..104` health-receipt digest |
| `KELD-AY1` (candidate) | 8 | `0..8` magic |
| `KELD-AF1` (candidate) | 9 | `0..8` magic, `8` class: `1` bootstrap read refused, `2` application exit before Ready, `3` boot error |
| `KELD-AK1` (owner) | 9 | `0..8` magic, `8` result: `1` accepted, `2` rolled back |
| `KELD-BH1` (helper; S11) | 77 | `0..8` magic, `8` purpose `2`, `9..41` installation ID, `41..73` client nonce, `73..77` client PID |
| `KELD-BQ1` (host; S11) | open (S11) | `0..8` magic, then "the hello's binding" (the `BH1` fields that S11 names), server nonce, server PID and bounded source locator, in `LC1` order |
| `KELD-BA1` (helper), `KELD-BR1` (host); S11 | 113 + locator | `0..8` magic, `8` purpose `2`, `9..41` installation ID, `41..73` client nonce, `73..105` server nonce, `105..109` client PID, `109..113` server PID, `113..` bounded source locator (encoding fixed by S11) |
| `KELD-BO1` (helper; S11) | 9 | `0..8` magic, `8` closed outcome class (values fixed by S11) |

*Transcript.* (approved: KEL-270 owner decision `eff8e2fb`, 2026-10-06)
`AA1` acknowledges the whole transcript, the fields of `AH1` and `AC1` together. `AR1`
is its same-context receipt, as `LR1` is for `LA1`: the two share one layout, differ
only in magic, and each receiver compares the whole record with the one it expects
(`bootstrap.rs:1506-1600`). That is what makes the exchange bind all three IDs, both
nonces and both PIDs (*Acceptance*). The owner accepts `AH1` only when its installation
ID is the owner's own and its client PID is the connected client's process ID. The
claimant accepts `AC1` only when its server PID equals `GetNamedPipeServerProcessId` and
the locator check passes. The owner accepts `AA1` only when it equals the record that
the owner builds from the accepted `AH1` and its own `AC1`; it then consumes its
one-shot and sends `AR1`, which the claimant requires exactly. The bootstrap differs
only where its field lists do: `BA1` acknowledges the whole bootstrap transcript, the
fields of `BH1` and `BQ1` together, and `BR1` is its same-context receipt. The host
accepts `BH1` only when its client PID is the process ID of `hProcess` ("Machine-UAC
bootstrap" item 2) and its installation ID is the host's own. The helper accepts `BQ1`
only when the `BH1` fields that it repeats equal its own `BH1` and its server PID equals
`GetNamedPipeServerProcessId`; S11 fixes which fields those are (*Bootstrap records*).

*Nonces and purpose.* (approved: KEL-270 owner decision `eff8e2fb`,
2026-10-06) Each client and server nonce is 32 bytes drawn for one connection from the
landed `keld-ipc` `SessionToken` generator (`token.rs:10`, `:49`), the reused nonce
utility (criterion 17). The bootstrap nonce comes from the same generator once per
bootstrap endpoint, is a locator input only, and appears in no record, because no field
list names it. Only the bootstrap records carry a purpose byte, because only the `BH1`
field list names one. The claim records carry none: their magic separates them from the
bootstrap records, and the locator purpose separates their endpoint names.

*Health records.* (approved: KEL-270 owner decision `eff8e2fb`,
2026-10-06) `AB1` carries the fields of the §4 health receipt: the attempt, the health
channel and the artifact. Criterion 8 and Architecture 06 §4a "Health identity", as this
amendment rewords them, require the receipt to bind the attempt ID and the full artifact
identity through the §4 receipt digest. The last field of `AB1` is that health-receipt
digest (`records.rs:363-378`) over the attempt ID, the health-channel ID and the
candidate artifact identity that the candidate matched to its own version tree
(*After acceptance*). The owner recomputes that digest from the journal, refuses a
mismatch, and on exact health `HealthAccepted` records the same value. `AY1`, `AF1` and
`AK1` carry only what their field lists name and repeat no ID: they travel on the
connection whose acceptance consumed the one-shot before `AR1`, and its handles are
non-inheritable and in no role's handle list (*Creation*, *Claim*).

*Health sequence.* (approved: KEL-270 owner decision `eff8e2fb`, 2026-10-06) A reader
reads the 8-byte magic first, refuses a magic that is not admitted at that position, and
only then reads the rest of that record under its deadline. After `AR1` the owner admits
`AB1`, or `AF1` with class `1` or `3`; after `AB1`, `AY1`, or `AF1` with class `2` or
`3`; after `AY1`, no record. The candidate sends nothing after `AY1` or `AF1` and reads
exactly one `AK1`. In candidate mode the host neither arms its landed recovery gate at
`Ready` nor treats the revocation of an application generation after `Ready` as
recoverable until it reads `AK1` accepted: from its `AY1` until then it handles such a
revocation as it already handles one before `Ready` (`keld-core`
`app_session.rs:5129-5134`): it denies the gate, which then provisions no successor
(`role.rs:731-744`), and ends the host, so the owner observes end of file or its
signaled launch handle. On such a revocation the host first closes its attempt
connection, then denies its gate and ends. The owner accepts health only when, 30
monotonic seconds plus a fixed margin G after it reads `AY1`, its launch handle is
unsignaled, the connection open and no further byte received. G is a constant that S6c
fixes from the host's measured revocation-to-close latency and records with that
measurement; an exit whose latency exceeds G can still commit, a residual that the
in-band alternative shares. A malformed, truncated, out-of-position or mismatched
record, end of file, a signaled launch handle or an expired deadline ends the exchange:
before `AR1` it refuses the claimant (*Refusal*); after `AR1` it cannot commit health
(criterion 8), and the owner rolls back. The owner sends `AK1` accepted only after
`HealthAccepted` is durable, because an owner lost before that write leaves
`AwaitingHealth`, which recovery rolls back. On the accept path the owner, after it
writes `AK1` accepted, waits under a deadline on its landed deadline-bounded read
(`windows_named_pipe.rs:757-764`, `:889-894`, `:1230-1239`, `:1316-1320`, `:1363-1369`)
for the candidate's end of file before it disconnects or closes the endpoint, so
`DisconnectNamedPipe` never discards an unread `AK1`; the candidate closes its attempt
connection after it reads `AK1`. The candidate reads `AK1` under a deadline; on end of
file, a read failure, that deadline or `AK1` rolled back it never arms its gate, so any
later generation exit ends the host. On the rollback path, while the connection is still
open, the owner sends `AK1` rolled back once and then ends the candidate family without
waiting for the candidate to read it; after end of file or a signaled launch handle it
sends none. A failed `AK1` write changes neither outcome. On the accept path, once the
candidate's end of file or that read's deadline has been reached, the `PerUserDirect`
owner continues with "Candidate release after commit" below.

*Bootstrap records.* (approved: KEL-270 owner decision `eff8e2fb`,
2026-10-06) `BH1` to `BO1` land with S11, not S4 (§6). S6 and every `PerUserDirect` cell
use none of them. Before S11, S1's stop rule can still change the bootstrap. Two of
their fields also lack an approved definition, and one phrase has two readings. KEL-53
("Machine-UAC bootstrap" item 5) and KEL-254 criterion 14 name the bounded source lookup
locator but fix no grammar, bound or user-side staging-root layout. The `BO1` field list
names only "a closed outcome class". "The hello's binding" in `BQ1` can name all four
`BH1` fields, which lets the helper check its own nonce and process ID one message
before `BA1`, or only the purpose and the installation ID, as the landed
`WindowsLifecycleBinding` (`bootstrap.rs:784-789`) and the strict field lists above read
a binding. S11's wire review fixes the source-locator encoding, which also fixes the
size of `BQ1`, `BA1` and `BR1`, and keeps these records fixed-size. The same review
fixes the `BQ1` field set and the `BO1` class values.

*Proposals.* (approved: KEL-270 owner decision `eff8e2fb`, 2026-10-06) The approved
field lists above leave the following values and rules open. Each was this amendment's
proposal for the wire review and is approved with it; its rationale follows:
- the locator's BLAKE3 hash and NUL-terminated `keld.<name>/v1` domain, the style of the
  landed domain-separated derivations (`records.rs:22-25`, `:337-378`; `keld-pack`
  `expected_identity.rs:16`); its purposes `1` and `2`, numbered from `1` as the landed
  lifecycle purposes are (`bootstrap.rs:815-820`) but in their own closed type; the zero
  `b` of purpose `2`, which keeps one input shape and one encoder; and the refusals of
  the landed lifecycle binding;
- the locator's single owner, `keld-ipc`, and its Windows-only `blake3` edge, for the
  `Cargo.toml` facts under *Locator*; S4 lands purpose `1` beside `is_attempt_endpoint`,
  with one shared prefix constant, and S11, the first consumer of purpose `2`, lands it;
- the 32-byte nonce width, that of `SessionToken` and of the `LC1` nonces;
- the `LC1` field order, and `AR1` and `BR1` as copies of the transcript that they
  confirm, as `LR1` copies `LA1`;
- the class and result values, numbered from `1` in their field-list order so that a
  zeroed byte never decodes, and the one `BH1` purpose value `2`, the bootstrap locator
  purpose: the recovery role uses no bootstrap, so one value suffices, and one set of
  purpose values serves the locator and `BH1`;
- the health-receipt digest as `AB1`'s artifact field, because the canonical artifact
  identity varies in length and the digest commits to all of it at a fixed size through
  the derivation that `HealthAccepted` already records; criterion 8 and Architecture 06
  §4a "Health identity" therefore read "binds the attempt id and the full artifact
  identity through the §4 receipt digest" instead of "repeats", under this same
  approval;
- the magic-first rule (*Health sequence*): a record's length follows from its magic, so
  no record carries a length field, and a reader refuses an unexpected record before it
  reads any other byte of it;
- the position-admission rule (*Health sequence*), which follows the approved order: the
  application starts only after `AB1` (*After acceptance*), so a refused bootstrap read
  (class `1`) can only precede `AB1` and an application exit before Ready (class `2`)
  can only follow it, a boot error (class `3`) can come on either side, and after `AY1`
  the window needs no record;
- the candidate-mode generation-exit rule and the `AK1` timing (*Health sequence*). The
  owner cannot see a generation exit, because the host replaces Bun generations
  in-process (Architecture 06 §1), and §5 plans no Job-notification path. The rule moves
  two landed decision points to `AK1` accepted: the arm of the `keld-runtime`
  `RoleRecoveryGate` (`role.rs:40-47`), which only holds successor provisioning while
  undecided (`role.rs:731-744`), and the `keld-core` predicate that makes a revocation
  terminal, today `window_ready` (`app_session.rs:5129-5134`), which the host sets at
  `Ready` just before it requests the arm (`app_session.rs:4561-4562`); before the first
  bind keld-core already denies on its own (`app_session.rs:3253-3260`). Deferring the
  arm alone leaves the host alive with no generation and lets the owner commit. With
  both moved, a process exit that the kernel reports carries the failure, with no new
  wire value, and the landed `ProcessCrash` class (`records.rs:187-188`) records it. As
  under the alternative, the surviving host must act on the revocation, and its
  revocation-to-close latency G is a window edge that both options share. `AK1` accepted
  becomes load-bearing for in-process recovery, so the owner waits under a deadline for
  the candidate's end of file before it disconnects, and the candidate reads it under a
  deadline; this reuses the landed read and adds no FFI. It moves recovery from such an
  exit from an in-process successor to the attempt owner's rollback, a crash-ownership
  change that root AGENTS.md treats as architecture, so Architecture 06 §4a states it
  under this same approval. The alternative, a fourth `AF1` class for an exit after
  Ready, admitted until `AK1`, keeps in-process replacement but adds a class that the
  approved list does not name, and health then commits whenever the surviving host does
  not deliver that record within G, as under this rule when it does not close.
  Owner-side attempt-Job completion-port notifications are rejected: their delivery is
  not guaranteed (JOBOBJECT_ASSOCIATE_COMPLETION_PORT, `ms.date` 2018-12-05), they need
  new `keld-runtime` FFI, and the general exit message cannot tell a host-authorized
  exit from a crash;
- the window start at the owner's read of `AY1`, because the owner cannot observe the
  candidate's own `Ready` time without trusting it, and the fixed margin G after the
  window, because an exit in its last G would otherwise reach the owner only after the
  commit;
- the S11 placement of the bootstrap records and of the `BQ1` field set
  (*Bootstrap records*).

(approved: KEL-270 owner decision `eff8e2fb`, 2026-10-06) Rejected:
repeating the `AH1` fields in `AC1`, and the attempt and health-channel IDs in `AY1`,
`AF1` and `AK1`, as the lifecycle challenge and receipts do (`bootstrap.rs:1471-1504`;
`keld-runtime` `windows_job.rs:2780-2791`, `:2958-2968`). No field list names them, and
the exchange binds without them.

Clients reject the other `\\.\pipe\keld-*` namespaces before connecting, which is the
`keld-ipc` rule for a separate-version protocol; Architecture 02 points here.

**Candidate release after commit (`PerUserDirect`; criteria 8 and 9; slice S6b3).**
(KEL-270 owner decision `740998f4-9a47-4527-9e1b-1adb10f4836e`, 2026-10-07, item 2: a
committed, healthy `PerUserDirect` candidate outlives the old host. Reply `559c07ac`
gives the owner's rule, a native qualification of option A first, else option B: "the
host clears kill-on-close on its own host-death Job just before it exits, after commit
and after reaping its roles". Reply `079b239d` records that option A failed its
qualification, so B applies. The design below is the coordinator's, made under the
owner's delegation of 2026-10-08 for long-term robustness, on the native qualification
of B of the same day; this amendment's pull request records that evidence and its
hashes.) This paragraph owns the release: its facts, order, primitive, census, crash
table, rollback, supervision after release, claim scope and rejected alternatives.

*Facts.* Every no-argument launch of a Windows `keld-host` installs one unnamed,
non-inheritable, non-breakaway, kill-on-close host-death Job, H, before any listener,
child or window (the `--hello` diagnostic window runs without it), and today forgets
its only handle so that the kernel closes it at termination
(`crates/keld-host/src/main.rs:101`; `crates/keld-runtime/src/windows_job.rs:3209-3249`,
the forget at `windows_job.rs:3248`; the KEL-78/T3 contract of Architecture 06 §1). In
`PerUserDirect` the attempt owner is that host. It creates the candidate
`CREATE_SUSPENDED` under its own token with no breakaway flag
(`crates/keld-runtime/src/windows_lpac.rs:497`) and assigns it to the unnamed
kill-on-close attempt Job, A, before its first instruction (S6b). A child belongs to
every Job in its parent's chain, and closing the last handle of a kill-on-close Job ends
the processes of that Job and of its nested Jobs
([Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects),
[Nested Jobs](https://learn.microsoft.com/en-us/windows/win32/procthread/nested-jobs),
both `ms.date` 2025-07-14), so the candidate is a member of A and of H. The native
qualification of B, three identical runs on build 26300 of a harness that mirrors
`keld-runtime`'s Jobs, showed: with neither Job cleared, with only A cleared and with
only H cleared the candidate dies with the host; with both cleared it survives the
host's exit; `JobObjectBasicProcessIdList` on H lists every member, and per-member
`IsProcessInJob(A)` tells the family from the rest; a console child's `conhost.exe` is
created asynchronously after the child resumes and inherits its creator's Jobs; a
member's removal from the list can lag its termination; and 64 nested levels and 10
update generations assigned without failure. Both clears are necessary, and together
they are sufficient.

*Handle ownership.* `install_host_death_job` returns an opaque capability,
`WindowsHostDeathJob`: not `Clone`, no raw-handle accessor, the handle non-inheritable
and held in `ManuallyDrop`, so that dropping the value never closes it. Nothing closes
the handle while the host is alive, which is the KEL-78/T3 invariant unchanged, and an
abnormal host death still closes it through the kernel, as today. It exposes the landed
`WindowsHostJobObservation` through an accessor and one consuming operation,
`release_for_exit`:

```rust
/// The host's own host-death Job. Not `Clone`; no raw-handle accessor; the handle is
/// `ManuallyDrop<OwnedHandle>`, so dropping this value never closes it.
pub struct WindowsHostDeathJob { /* private */ }
impl WindowsHostDeathJob {
    pub fn observation(&self) -> WindowsHostJobObservation;
    /// The census, then the host-death clear; consumes the capability, never closes it.
    pub fn release_for_exit(
        self,
        released: &WindowsReleasedAttempt,
        deadline: Instant,
    ) -> Result<WindowsExitCensus, WindowsHostJobError>;
}
pub fn install_host_death_job() -> Result<WindowsHostDeathJob, WindowsHostJobError>;
impl WindowsProcessJob {
    /// The attempt-Job clear; exclusive by type with `terminate_and_wait`.
    pub fn release_family(self) -> Result<WindowsReleasedAttempt, WindowsHostJobError>;
}
/// The attempt Job after its clear read back `0`; keeps the handle for membership
/// queries only.
pub struct WindowsReleasedAttempt { /* private */ }
/// Census witness for the §7 rows.
pub struct WindowsExitCensus {
    pub family: u32,
    pub terminated: u32,
    pub snapshots: u32,
}
```

*One clear primitive.* `keld-runtime` has one crate-private clear beside
`create_process_job` (`windows_job.rs:2620`): query the extended limits, strip
`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, set them, and read the flags back through the
landed `query_job_limit_flags` (`windows_job.rs:2673`); any read-back other than `0` is
"not released", a typed `WindowsHostJobError`. Two consuming entries share it.
`WindowsProcessJob::release_family(self)` clears A and returns `WindowsReleasedAttempt`,
which keeps A's handle for membership queries only; it and `terminate_and_wait`
(`windows_job.rs:2210`) both consume the Job, so by type a rolled-back attempt is never
released and a released attempt is never terminated.
`WindowsHostDeathJob::release_for_exit(self, &released, deadline)` runs the census and
then clears H; it cannot be called without a released attempt, so H is never touched
unless A was released first. Machine-UAC shares only the primitive: the elevated helper
calls `release_family` once `health-accepted` is durable and keeps its own timing
("Machine-UAC owner-loss retirement"; S11).

*Order.* On the accept path the `PerUserDirect` owner proceeds in this order, each step
only after the previous one has returned: (1) `HealthAccepted` is durable
(`accept_health`, `crates/keld-update/src/windows_baseline/activate.rs:415`); (2) it
writes `AK1` accepted and reaches the candidate's end of file or that read's deadline
(*Health sequence*); (3) `complete()` publishes both known-good slots, retires the
superseded version and removes the journal (`activate.rs:487`); (4) `release_family`,
only after `complete()` returned `Ok`: on `Err` the host exits with both Jobs still
kill-on-close, the candidate dies with it, and the `HealthAccepted` journal stays
authoritative, as at W0b (availability only); (5) the host reaps its own roles,
through the landed WebView2 `BrowserProcessExited` barrier
(`crates/keld-wv/src/webview2/mod.rs:1229-1260`, its deadline at `mod.rs:186`) and the
Bun teardown of the accepted-shutdown tail; (6) the census; (7) the H clear; (8) exit.
Steps 6 and 7 are `release_for_exit`. Releasing before `complete()` is
rejected: an owner lost between the release and journal removal would leave a
`HealthAccepted` journal beside a running, released candidate; `recover` needs an exact
process-family retirement binding for every launched phase (`activate.rs:603-627`),
which no owner can compose while that family runs, so recovery would halt for as long
as the candidate lived.

*Census and policy.* The census takes a snapshot of H's process IDs with
`QueryInformationJobObject(JobObjectBasicProcessIdList)`; a list shorter than the
assigned count is retaken with a larger buffer. For every ID other than the host's own
it opens the process with `PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE |
PROCESS_SYNCHRONIZE`, one handle per member, through which it classifies, waits and, if
needed, terminates; a second open for termination would be a second lookup of a
reusable ID. An open that fails because no live process has that ID
(`ERROR_INVALID_PARAMETER`: the member exited after the snapshot; S6b3's evidence
confirms the status) means a new snapshot; any other open failure refuses. It
classifies each opened member first by `IsProcessInJob(H)`: not in H means the ID was
reused by a process outside H, so the member is skipped and a new snapshot taken; then
by `IsProcessInJob(A)`: in A is family, which covers the candidate, every process it
starts, its own host-death Job and, by construction, its console host, when the launch
allocates one. Every other member is outside the family: the host's Bun primary and
any descendant it left, since Bun receives no inner Job and is reaped only by H
(`crates/keld-runtime/src/lib.rs:1221`, `lib.rs:1446-1450`); a WebView2 process past
the barrier; and any console host that a member of H allocated (the host's own console
host predates H and is not a member). Terminating a WebView2 process past the barrier
assumes that no host WebView2 process serves the candidate: WebView2 ties every process
of a user data folder to that folder's one browser process, shared across the
processes that open it
([Process model for WebView2 apps](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/process-model),
`ms.date` 2022-04-01), and the KEL-135 profile lease (`profile.lock`,
`crates/keld-wv/src/webview2/mod.rs:188`) is opened with no sharing and refuses a
second opener with `ProfileInUse` rather than sharing (`mod.rs:521-538`), so the
assumption holds exactly when the candidate mode never opens the user data folder that
the old host holds; S6c owns that rule for its candidate mode (§6). The census
terminates each member outside the family through the handle it opened, with the exit
code `1` that the attempt Job's termination uses (`windows_job.rs:2193`),
treating a member whose handle is already signaled as terminated, then waits for their
handles under the deadline and takes a new snapshot; it repeats until a snapshot shows
only the host and family, or until the deadline. A `TerminateProcess` failure on a live
member, such as access denied, is not refused on its own: the member stays in the next
snapshot, and the deadline refusal covers it. This is exactly what closing H at exit
would do to those members, moved before the clear, and it is what makes the clear safe:
after a clean snapshot no process outside the family exists in H that could start
another, other than the host, which starts nothing after the census. At the deadline
it refuses: `release_for_exit` returns the typed refusal, the host exits with H still
kill-on-close, the candidate dies with it, and the next launch
boots the committed version, since every record is already committed; the refusal is
availability-only. S6c measures the deadline from the reaping latencies its PR records
and fixes it there. Rejected: comparing H's member count with A's (a member can leave
and another join between two reads); matching image names (a reused ID or a renamed
image); refusing on any member outside the family at once (WebView2 and Bun stragglers
would make release almost never succeed); and waiting without terminating (one stray
Bun descendant would defeat release in every app).

*Crash table.* An owner lost at W0a, after `HealthAccepted` is durable and before `AK1`
accepted, or at W0b, after `AK1` accepted and before `complete()` returns, leaves what
it leaves today: the candidate dies with the host through A and H, the journal still
reads `HealthAccepted`, and recovery needs the retirement binding (refused with
`JournalBoundRecoveryRequired` before S6d). An owner lost at W1, after `complete()` and
before `release_family`, at W2, after `release_family` and before the H clear, while
reaping or in the census, or at W4, where the census refuses at its deadline, leaves no
journal: the candidate dies with the host and the next launch boots the committed
version, availability only. After W3, both clears done, the candidate survives whatever
the host does next. On rollback, `terminate_and_wait` consumes A, so `release_family`
cannot be called, H keeps kill-on-close, and nothing is cleared.

*Supervision after release.* The released candidate is itself a `keld-host`: before any
child it installs its own host-death Job, nested under A and H, so its own abnormal
death reaps its roles as today. S6c keeps that install first on the rendezvous-argument
path; today `crates/keld-host/src/main.rs:85-90` refuses every argument before the
install at `main.rs:101`. Each in-session update nests two further Jobs under the
previous ones. On the qualification's 64 levels and 10 generations that growth is
accepted; the §7 row repeats ten in-session updates.

*Claim scope and residuals.* Only this host's own two Jobs are cleared. An outer Job
that bounds the host, such as a launcher's or CI's kill-on-close Job, still bounds the
candidate, and nothing here claims otherwise. Nothing in Keld hands H to another
process; a same-user process that duplicates the handle out of the host
(`PROCESS_DUP_HANDLE`) can join H, and what it does to the candidate is outside the
`PerUserDirect` boundary (criterion 1). A keeper or a criterion-10 post-exit helper
that the host starts inherits H and is outside A: B does not cover it, and the census
would terminate it as a member outside the family. S6d's own specification solves that
at the root before S6d starts (§6, §10).

*Rejected alternatives.* Option A, breakaway from H: `JOB_OBJECT_LIMIT_BREAKAWAY_OK` is
a property of the whole Job, and the qualification showed that any direct member, the
Bun primary and the WebView2 processes included, could then create a child that leaves
H (reply `079b239d`). An out-of-process relauncher or broker outside the Jobs: it adds
a long-lived process and principal outside Keld's supervised family and loses the
suspended-launch, assign-before-first-instruction binding that S6b proved. A keeper
that retains the Job handle: rejected above for Machine-UAC, for the same reasons.
Releasing before `complete()` (*Order*). A type witness from `complete()` on
`release_family`, which would make step 4's order a compile-time fact as
`release_for_exit`'s is: `keld-runtime` cannot name a `keld-update` type, because
Architecture 01 §3 owns the dependency direction and gives `keld-runtime` only
`keld-ipc` (`docs/architecture/01-overview.md:99-100` and `:111`). `keld-update`
already sits above `keld-runtime` through a Windows dev-dependency (its tests,
`crates/keld-update/Cargo.toml:31-34`); that edge alone would not block the reverse
edge, because Cargo allows a
[dev-dependency cycle](https://doc.rust-lang.org/cargo/reference/resolver.html#dev-dependency-cycles),
so the architecture rule, not the manifest, is the blocker. S6c's composition and the
r8 and r12 cells of the §7 row prove the order instead. Clearing without a
census: the clear releases every member of H, so a Bun descendant or a WebView2
straggler would outlive the host unsupervised.

### Trust, package and channel ownership

The semver floor is the only v0 replay/downgrade floor. It advances before candidate
publication and never rolls back. `current` may point below it after health
failure; that is intentional local rollback, not permission to reinstall an old release.

The installer synchronizes the immutable baseline package, seeds floor/current/LKG to
that exact artifact, then creates immutable provenance as its final commit. `Direct`
records exact identity/channel/target/roots/key/baseline, install mode, owner and its
mode-specific OS protection profile. `Managed(mechanism)` always refuses direct
mutation. Missing provenance is unsupported. Location, registry heuristics and
writable config never manufacture a mode or `Direct` ownership.

`PerUserDirect` is the default and stores the application and updater state beneath
the owning user's LocalAppData tree. Its updater uses the ordinary user identity and
does not display UAC; strict Keld role restrictions deny role access to update state,
while the security claim excludes the owning user and arbitrary native malware running
as that user. `MachineUacDirect` stores machine-wide state in a Program Files-style root
whose installer-provisioned DACL allows elevated Administrators and SYSTEM to mutate,
and ordinary users/Keld roles only to read/execute. It is not KEL-266's SYSTEM-only
profile and never repairs ownership/DACL during an update. `MachineSeamlessDirect` is an
opt-in SYSTEM-protected profile whose narrow writer mechanism remains a separate proof
gate. `Managed(mechanism)` performs no direct feed, stage or protected write.

The product and ownership boundary is explicit: KEL-135 owns verified publisher/app/
profile identity; KEL-53 owns install-mode provenance, update journal, active-package
selection and the sole writer; KEL-254 owns OS verification of installed-image and
protection profiles plus the read-only selection boundary, never write authority;
KEL-96 owns host boot admission and consumes the exact opaque selection as the ordinary
user, without inferring install mode or mutating updater state. KEL-254's approved
installed-root spec defines the per-user and Machine-UAC protection profiles; their
native implementation and verification remain required before those direct modes ship.

Windows x64 direct distribution is first because its tree fits v0. KEL-137 is an
explicit predecessor for macOS/Linux or any package requiring metadata absent from v0.

### T3b: protected Windows extraction (KEL-265)

This task produces an unpublished, incomplete stage, not an activated version. It
does not supply the installer provenance loader, live strict-profile admission,
persisted-floor loader or T4 publication/recovery. The first filesystem cell is
Windows x64, fixed local NTFS with persistent ACLs and no read-only volume flag.
Failed/unknown qualification refuses; there is no pathname or filesystem fallback.
`GetVolumeInformationByHandleW` observes the retained root, and
`GetFinalPathNameByHandleW(VOLUME_NAME_GUID)` supplies the exact volume root for
`GetDriveTypeW`. These observations do not prove physical non-removability or
power-loss durability.

The host-facing API has opaque, non-cloneable owners:

```rust
impl AdmittedInstallation {
    pub fn open_windows_extraction_root(&self)
        -> Result<WindowsExtractionRoot, UpdateError>;
}
impl WindowsExtractionRoot {
    pub fn extract<'a>(&'a mut self, verified: &VerifiedFull, archive: &Path)
        -> Result<ExtractedWindowsStage<'a>, UpdateError>;
}
```

The root opener derives `update_root` from the admitted installation and creates
nothing. It retains no-follow directory handles through every accepted absolute
path component, with delete sharing disabled. Both `update_root` and its existing
`versions` directory must be actual non-reparse directories with the current
TokenUser owner and exactly one protected, non-inherited OI/CI full-control allow
ACE for that user. Missing scaffolding or a different descriptor refuses; extraction
does not repair ACLs or initialize installation state. Logical
`ProvenanceObservation::Protected` is not evidence for these OS facts.
Existing installer path components use the guard's ordinary filesystem grammar,
including serviceable tilde names. The stricter package namespace grammar applies
to archive members and newly created package directories, not ancestor root names.

`SelectedFull` and `VerifiedFull` privately retain the admitting
`DirectInstallationIdentity`. Before opening the source or creating a stage,
extraction reuses the provenance owner's complete identity comparison, including
key, roots, baseline and profile, and requires candidate SemVer precedence above
the receiving root's admitted floor. Equivalent verifier instances may interoperate;
different admission contexts may not. This floor is a snapshot: T4 must reread the
protected floor under its single-writer lock before publication.

The source is an internally opened, unexposed read-only file with only read sharing.
It must be regular, non-reparse and single-link. Retain it across complete canonical,
namespace, policy and digest preflight and every source read; do not combine a
detached preflight receipt with a caller-controlled reader. Complete preflight
precedes creation. A fresh unpredictable stage name is created exclusively beneath
retained `versions`; collisions refuse rather than reusing or repairing an object.

Directory creation uses one private, directory-only `NtCreateFile` adapter with a
retained `RootDirectory`, one validated component, `FILE_CREATE`,
`FILE_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT`,
`OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE`, no handle inheritance and no delete
sharing. A shared owner-private descriptor is supplied atomically. The successful
handle passes metadata and ACL checks before any descendant creation. Safe pinned
capability file operations provide create-new/no-follow regular files beneath those
retained parents; every file is checked for type, reparse state, single link and the
exact inherited current-user full-control ACL before its first write.

`keld-guard` becomes the single owner of the existing CLI/core owner-private
descriptor construction and validation. Core retains its public path wrapper and
CLI retains atomic dev-stage creation; neither retains a second ACL policy.
The updater reuses this owner and the guard package namespace validator.
`cap-std`/`cap-fs-ext` file primitives are reused. Their Windows mkdir resolves a
pathname, so it cannot establish confinement when an ancestor's reparse attributes
are mutable. Delete-sharing pins alone do not restrict attribute or ACL changes.
The narrow native directory adapter replaces only that missing primitive, using
pinned `windows-sys` bindings rather than a copied ABI or generic filesystem layer.

Retained `content.tar` is a sibling of `tree`; archive paths address only `tree`.
Stream bounded chunks, flush every written file, close its writer, reopen it relative
to its retained parent and read it back against authenticated source bytes. Verify
object identity, length and protection on readback and retain read handles with no
write/delete sharing, together with every created directory, for the stage lifetime.
The returned stage borrows the exclusive root owner and exposes identity/diagnostic
name only, not raw handles or a trusted pathname capability. It creates no `.complete`,
final version name, floor, journal or pointer. T4 must consume the stage and perform
the separately qualified publication sequence. Any failure after stage creation
returns a typed error naming the incomplete stage; preserve that bounded diagnostic
state without recursive cleanup or a success receipt.

The independent atoms are: admission identity (context substitution refuses before
source/stage I/O), byte authentication (locked-source mutation refuses), authorization
(permissive root/versions and hostile LPAC writes refuse), containment (single-component
relative create cannot affect an outside sentinel), lifecycle (flush/readback failure
never returns a stage), and evidence (real Windows handles/token plus exact source).
ACL protection and retained-object lifetime are an explicit composition edge, not
interchangeable proofs. Tests include matched successful mutation controls, source
writer/mapping conflicts, hardlink/reparse/collision refusal, released-pin rename,
and temporary mutations of context, ACL, relative-create and readback checks. No
sleep synchronization and no shipping installer/activation claim arise from fixtures.

The implementation may add existing workspace-pinned `cap-std`, `cap-fs-ext`,
`windows-sys`, `windows-permissions` and `getrandom` consumer edges, plus Windows-only
test dependencies for existing LPAC launch primitives. It introduces no crate/version,
manifest field, application grant or KIPC change. The updater's exact new unsafe path
requires its own scoped instruction owner and independent review; no crate-wide
unsafe permission is granted.

### T4a: protected Windows baseline bootstrap (KEL-266)

This bounded amendment implements the machine-baseline portion of acceptance criterion 1 before activation. The
installer is a one-shot, externally provisioned LocalSystem process; the ordinary
host receives read authority only. It adds no service, elevation path, recovery,
health, activation selection or role grant. KEL-254 still owns installed boot and
must compare these protected publisher facts with KEL-135's independently verified
current-image identity. Its fresh-role read provisioning remains separate. This is the
KEL-266 `MachineSeamlessDirect` baseline predecessor only; it does not define the
default per-user bootstrap or the distinct Machine-UAC ACL/helper contract.

| Atom / owner | Boundary and observable contract | Independent falsifier |
|---|---|---|
| Identity / updater | Trusted installer/host configuration binds installation, publisher scope and volume GUID | Change one expected field; no receipt |
| Authentication / manifest and full verifier | Literal signed manifest selects the exact configured baseline and fully verifies its package | Wrong signature, full version string or digest; zero publication |
| Authorization / guard | Actual initializer TokenUser is SYSTEM; caller configuration is trusted deployment input, never an elevated feed/argv assertion | Ordinary or elevated non-SYSTEM token refuses |
| Persistent containment / guard | Every ancestor excludes ordinary-user replacement between invocations | Protected leaf under user-owned parent or parent DELETE_CHILD refuses |
| Lifetime containment / updater | Retained no-delete-share ancestors and relative creation bind live objects | Reparse, hardlink, rename, source-write and readback controls |
| Lifecycle / initializer | Exclusive fresh lock; exact initial seed state; provenance is last commit record | Existing state, competing initializer or pre-provenance crash produces no admitted installation |
| Evidence / native tests | Real token, descriptor, filesystem and subprocess observations | Logical fixtures never satisfy SYSTEM/user or crash-cut acceptance |

Signature verification does not prove writer authority; authority does not authenticate
arbitrary configuration; a matching record does not prove OS protection; completed I/O
does not alone prove power-loss durability. These atoms stay independently testable.

The first cell is Windows x64 on one qualified fixed local NTFS volume. Trusted
`WindowsBaselineTrust` holds the existing `DirectInstallationIdentity`, the existing
32-byte KEL-135 publisher scope and canonical volume-GUID root. Records encode that scope
as exactly 64 lowercase hexadecimal characters. Publisher scope is an installer
assertion, not an Authenticode result. It must originate in trusted deployment/host
configuration, not lower-trust environment, feed or arguments. The loader compares all
fields, including observed volume identity; the protected record cannot supply its own
expected trust anchor. For the KEL-254 executable-located path, that anchor is the
build-time `ExpectedAppIdentity`, the located roots' file identities and the recorded
mode's OS protection profile, and the record's other fields are accepted only after
those match. SYSTEM/admin volume restoration is outside the ordinary-user
replay threat; no global monotonic counter is introduced.

Supported paths are lossless UTF-8 absolute drive paths, optionally verbatim-drive,
with normal guard-validated components; UNC, device aliases, reparses, relative/dot
components, empty components and alternate separators refuse. `update_root` is one
direct child of `install_root`; `versions` is a direct child of `update_root`.
The externally provisioned initial install root contains only that update directory,
which contains only empty `versions`. Install/update/versions already have the exact
`windows-system-users-rx-v1` MachineSystem descriptor. No initializer creates, seals
or repairs the scaffold; it validates these committed roots before mutation.
Every earlier named ancestor below the volume root already has the committed machine
descriptor. No user-owned intermediate path (for example a development workspace) is
an admitted installation location.

The shared guard owns the committed profile `windows-system-users-rx-v1`: owner SYSTEM,
protected DACL, exactly SYSTEM full control plus BUILTIN Users file read/execute
(`0x1200a9`); directory ACEs have object/container inheritance, file ACEs have none.
Every object is explicitly sealed and read back; inheritance alone is not proof.
The volume anchor has a separate conservative predicate: trusted SYSTEM,
Administrators or TrustedInstaller owner; present DACL with only understood ordinary
ACE forms; effective allow ACEs for other trustees grant at most read/execute plus
creation of new directories. No untrusted DELETE, DELETE_CHILD, WRITE_DAC, WRITE_OWNER,
WRITE_DATA, WRITE_ATTRIBUTES, WRITE_EA or generic-write/all right is admitted.
Inherit-only entries do not grant access to the anchor; all descendants are independently
checked. The helper reports an unsupported anchor rather than changing a drive ACL.
Both initializer and loader validate persistent ancestry before trusting state and
retain opened components for the result lifetime. T3b owner-private policy is unchanged.

`BaselineVerifier` reuses the existing literal-signature/strict-manifest parser and
full/archive verification. Its separate opaque `SelectedBaseline`/`VerifiedBaseline`
receipts select the exact configured baseline version string and content digest without
inventing an admitted installation, protected observation or lowered floor. Ordinary
update selection still chooses only the highest release strictly above its floor.
Reuse T3b's source locks, canonical/policy parser, relative directory adapter, extraction,
flush and readback; do not add a second archive, signing or policy implementation.

One crate-private bounded (64 KiB) canonical UTF-8 JSON codec owns local records. Each
record has a distinct explicit v1 schema; unknown/duplicate/missing fields, unsupported
schemas and bytes differing from typed reserialization refuse. `install-provenance`
under the install root records direct ownership, protection profile, complete existing
installation identity, publisher scope and volume GUID. `.complete` records exact
artifact identity and content size. Under the update root, `version-floor` records
the exact baseline version, while `current` and `last-known-good` record the complete
baseline artifact. No previous-known-good or activation journal exists initially.
The version directory name is the complete validated baseline version string.

The only successful initialization order is:

1. Prove actual SYSTEM authority, topology, volume, exact MachineSystem descriptors and
   fresh state; create `bootstrap.lock` exclusively with its exact MachineSystem
   descriptor at object creation and retain its handle. Existing lock/state refuses;
   no PID guessing, takeover or stale cleanup. Seed the persistent `activation.lock`
   with that same exact descriptor and exclusive sharing.
2. Fully validate the authenticated exact baseline before extraction; create one fresh
   incomplete sibling and populate it with shared T3b mechanics.
3. Seal/read back each owner-private stage directory bottom-up. Payload files and
   `content.tar` receive their final MachineSystem descriptor before the original
   writable handle's final flush and protected readback. Create `.complete` with its
   final MachineSystem descriptor, write and flush it, then verify its exact bytes and
   descriptor.
4. Close rename-blocking stage handles while retaining protected ancestors. Publish to
   the absent final version name using same-volume `MoveFileExW` with only
   `MOVEFILE_WRITE_THROUGH`; no replacement or cross-volume-copy flags. Reopen and
   validate all content, policy, marker, descriptors and exact extracted-tree bytes.
5. Seed floor, current and LKG in order: each uses a fresh same-parent temporary file,
   final protection, writable-handle flush, close, absent-target write-through rename
   and protected readback. Any conflicting target or incomplete prior state refuses.
6. Revalidate the unchanged install/update/versions descriptors and retained lock; do
   not rewrite already trusted scaffold ACLs. Publish protected provenance by that same
   file procedure LAST. Re-read through the production loader and separately validate
   the exact complete initial seed state before returning success.

Failures retain diagnostic incomplete state and never silently reseed. The bootstrap
lock may remain after a commit; it is not activation/recovery authority. The read-only
loader returns coherent initial-baseline identity/floor and retained read handles. It
neither chooses an active package nor grants mutation, recovery, live strict-profile
or role authority by itself.
The initializer's exact-baseline postcommit tree check is separate from future active
package selection and garbage collection.

**Admission-to-staging completion (KEL-266 AC4–6).** Before exposing its observation
to the existing updater verifier, the public baseline loader also validates the exact
baseline `version-floor`, `current`, `last-known-good`, protected baseline version
directory and matching `.complete`. Missing/corrupt/mixed metadata refuses there,
not only in the initializer's final check. Previous-known-good, journal or unknown
update-root state belongs to future activation/recovery and refuses this initial cell.
The persistent bootstrap lock may remain, but is never repair authority. Reuse/factor
the existing seed and completion-record owners; retain the pointer, marker and version
directory handles. This metadata admission does not rehash the entire runnable tree
or authenticate the current executable. The latter remain their existing owners.

The only additional names admitted under `versions` are diagnostic
`incomplete-<64 lowercase hexadecimal characters>` siblings. Validate each named
object as a non-reparse directory without following or selecting its contents; it is
never a runnable artifact or a source of identity/floor/completion. Other final
versions require the future activation predicate rather than directory inference.

`LoadedWindowsBaseline::into_windows_extraction_root(self)` consumes the real retained
loader owner and requires the actual SYSTEM token. It derives installation/floor and
the versions handle internally, accepting no caller identity, floor, root or logical
protected observation. One private closed extraction-authority variant retains this
machine owner; the existing T3b owner-private variant is unchanged. Conversion and the
extraction mutation boundary require actual SYSTEM and the exact committed machine
descriptor. The consumed loader's metadata/ancestor pins live through extraction.
Both variants call the same verifier, source locking, namespace and copy/readback
implementation with owner-private stage protection. No `.complete`, final version,
floor, pointer, journal, repair or activation is produced by this conversion/staging.

Native acceptance starts from real initialized state: existing `UpdateVerifier`
admission consumes the qualified observation, authenticates a signed higher full
release, and SYSTEM stages its exact bytes while every baseline record remains
byte-identical. Ordinary-user conversion refuses before source/stage I/O. Another
installation/key/profile receipt, a non-higher candidate or changed machine descriptor
refuses. Release every owner and prove that the only new output is a private incomplete
stage. Canonical wrong pointer/marker/floor values and missing records must make the
public loader refuse; valid metadata restored byte-for-byte is the positive control.

The real LPAC probe runs under an ordinary host after releasing installation handles.
Reuse runtime launch/token observation and the existing filesystem-probe owner; grant
read/execute to its disposable helper and write authority only within its disposable
role-private control directory, granting neither to the committed machine installation.
Verify known provenance, floor/current/LKG and
payload targets exist, exercise actual denied mutations, require granted role-private
controls to succeed, then re-read protected bytes/descriptors. This proves the exercised
LPAC write denial, not installed-role read provisioning or WebView2 acceptance.

Native tests cover actual non-SYSTEM refusal, SYSTEM success, standard-user reads and
write/WRITE_DAC/rename denial after all initializer handles close, parent substitution,
wrong publisher/volume, extra write ACE, changed marker/tree/seed records, concurrent
initialization and subprocess termination at every persisted boundary. The current
unelevated agent cannot claim SYSTEM acceptance: a concrete reviewed operator helper
must run it. API completion plus process crash cuts is not a power-loss claim; the
governing native filesystem qualification remains required before shipping this cell.

This adds no dependency version, manifest/KIPC field or app permission. Existing pinned
Windows APIs are reused. Only the fixed write-through absent-target publication adapter
extends updater production unsafe ownership, with independent unsafe/permission/public
API and local-record protocol review. No new guard unsafe is authorized. Rejected
alternatives are a same-user/elevated-owner writer (implicit owner WRITE_DAC), a general
broker/service (unneeded authority), copied signer/parser code (duplicate owner), and
logical protected fixtures as native proof. No existing public compatibility fallback
changes; unsupported native cells return typed actionable refusal.

### Capabilities, wire and errors

Application permissions cannot grant update authority. No KIPC, renderer bridge or
permission-manifest change is introduced.

Feed wire changes: none. `updates.json`, its detached signature, v0 fields and
delta parsing remain unchanged. Slice A chooses `full`. T4d's local wire changes, the
journal schema v2 and the `keld-attempt` subprotocol, are listed in §8.

`keld-update` owns typed provenance, verification, package, activation-state,
health, durability and recovery categories. `keld-pack` owns production
failures. CLI/compat render them without remapping. Exact `KELD-UPDATE-*`
codes land with emitting code because the registry rejects un-emitted codes.

## 5. Boundaries

Implement in:

- `keld-update`: provenance, verification, floor, journal, pointers, health and
  recovery;
- `keld-pack`: Windows v0 package and exact no-migration policy;
- existing host/runtime owners: private health channel and candidate lifecycle;
- trusted installer: immutable per-user/machine-UAC/machine-seamless/managed mode
  provenance and owner-specific protection profile;
- a minimal signed helper for explicit-UAC activation and, if separately approved, for
  locked-file publication (the criterion-10 post-exit helper, not used in
  `MachineUacDirect`);
- `crates/keld-updater-helper` (T4d): a new workspace binary crate that builds only
  `keld-updater-helper.exe` with its activation and recovery-only roles. It composes
  existing owners and holds no production `unsafe` (it denies `unsafe_code`): its
  normal edges are `keld-update` (transaction, recovery, selection and self-anchor),
  `keld-ipc` (the `keld-attempt` subprotocol and pipe server), `keld-runtime` (Job,
  launch and process identity) and `keld-guard` (token, logon-session and Authenticode
  wrappers). Nothing depends on it, and `keld-update` keeps only its dev edges to
  `keld-ipc` and `keld-runtime`. Its build script passes `/DEPENDENTLOADFLAG:0x800` and
  the static-runtime link arguments for this binary only; its loader hardening and
  edge-set checks are owned by "Helper launch and self-anchor". Rejected alternatives: a
  binary target in `keld-update`, which would make `keld-runtime` and `keld-ipc` normal
  `keld-update` dependencies and add the `keld-update -> keld-runtime` edge rejected in
  §4; and a second binary in `keld-host`, whose package graph includes the WebView and
  app runtime that criterion 17 excludes from the elevated helper;
- `keld-pack` (T4d canonical-content change): every Windows package carries exactly one
  `keld-updater-helper.exe` beside `keld-host.exe` in its tree, covered by
  `contentBlake3`, in every install mode, because the signed artifact is mode-agnostic;
- dependencies outside this spec: KEL-19's payload writer and container, which embed
  `ExpectedAppIdentity` in both `keld-host.exe` and `keld-updater-helper.exe`, and
  KEL-254 T3 Part B's `ExpectedAppIdentity::from_signed_image`;
- landed-code changes in `keld-update` (safe code; owner KEL-53):
  - the executable-located locator replaces its fixed `HOST` constant
    (`windows_baseline/locate.rs:22`) with the closed choice of `keld-host.exe` or
    `keld-updater-helper.exe` ("Helper launch and self-anchor"), which changes the
    public entry point's signature (slice S9a);
  - the rule that the verified signer's publisher scope and app id equal the recorded
    ones moves from `keld-core` (`require_recorded`, `app_session.rs:2292-2313`,
    including the app-id check at `:2303`) into `keld-update`, so that the host and the
    helper's self-anchor share one owner; the host's refusal moves from `KELD-WV-009` to
    a `keld-update` code in S9a's reserved range (slice S9a; Coordination record, Linear
    KEL-270 comment `7905ec8a-2529-4c23-90f4-878d315bc0e5`, 2026-10-07);
  - `WindowsActivationAttempt::accept_health` (`windows_baseline/activate.rs`) splits
    into a durable `HealthAccepted` step and the completion after it, so that the owner
    sends `AK1` accepted exactly after the durable write (*Health sequence*), a breaking
    public API change (slice S6b; same coordination record);
  - `WindowsExtractionRoot::begin_activation` (`windows_extraction.rs:557-561`),
    `WindowsRecoveryInspection::recover` (`windows_baseline/activate.rs:603-606`) and
    `resume_unlaunched` (`windows_baseline/activate.rs:645-647`) take the attempt owner's
    `&keld_guard::VerifiedWindowsImage` instead of a raw
    `coordinator_image_blake3: [u8; 32]`, so a caller can supply only an image that
    passed `keld-guard` verification. Each public entry point delegates to one
    crate-private function that takes that image's `&std::fs::File`
    (`VerifiedWindowsImage::file`, `crates/keld-guard/src/windows_authenticode.rs:114`)
    and derives the digest; the crate-internal transaction tests
    (`crates/keld-update/src/windows_baseline/tests.rs:23`, under `#[cfg(test)]`) call
    that private function with plain files, so `keld-guard` gains no test constructor
    for its verified type, which a feature flag could expose to release builds through
    Cargo feature unification. No production caller can compute that digest,
    because the one image-digest owner, `image_blake3` (`windows_baseline.rs:676`), is
    private to `keld-update`; `keld-update` now derives it from the handle, with the
    derivation and the journaled bytes unchanged. A breaking public API change (slice
    S6b2; KEL-270 owner decision `740998f4-9a47-4527-9e1b-1adb10f4836e`, 2026-10-07,
    item 3, which names `begin_activation`; the two recovery entry points carry the same
    raw digest and follow it, so that no caller computes one);
  - `next_activation_step` (`activation.rs`) gains the abandon intent of owner decision
    D1 (refined), behind a new public recovery-role entry point (slice S10). Under that
    intent only: `activation.rs:203` maps a published candidate at the prior floor to
    `RetireVersion` (not `AdvanceFloor`); `:204` maps a staged one to `AbandonAttempt`
    (not `PublishCandidate`); `:210` maps a published candidate at the candidate floor
    to `RetireVersion` (not `SelectCandidate`); `:211-212` map an unpublished one to
    `AbandonAttempt` (not the `CandidateUnpublished` refusal); `:215` maps
    `ResumeCandidateHealth` to `RestoreRollbackTarget` (not `EnterAwaitingHealth`); and
    `retirement_due` (`:137-152`) names the candidate for `PublishPending` only under
    the abandon intent and only when no pointer names it. Every mapping is unchanged
    under the ordinary intent;
- landed-code change in `keld-runtime` and its call sites (slice S6b3; KEL-270 owner
  decision `740998f4-9a47-4527-9e1b-1adb10f4836e`, 2026-10-07, item 2, under replies
  `559c07ac` and `079b239d`; "Candidate release after commit"): `install_host_death_job`
  (`crates/keld-runtime/src/windows_job.rs:3209-3249`) returns the opaque
  `WindowsHostDeathJob` capability instead of forgetting the Job handle
  (`windows_job.rs:3248`), a breaking public API change; the capability holds the
  handle in `ManuallyDrop`, so no caller can close it and dropping it changes nothing.
  Its callers change mechanically, binding the capability and reading the observation
  through its accessor: `crates/keld-host/src/main.rs:101`,
  `crates/keld-host/tests/no_flag_windows.rs:204` and
  `crates/keld-runtime/tests/windows_host_death_job.rs:2160`,
  `windows_host_death_job.rs:2187` and `windows_host_death_job.rs:2225`. The host keeps
  the capability until it exits; S6c adds the one `release_for_exit` call on the
  committed exit path, and no other caller releases it;
- the new T4d production FFI. Each call below lives in the named file of its existing
  owner, only after an issue-scoped amendment of that owner's AGENTS.md `unsafe` rule;
  calls that the owner already lists gain only the stated new scope:
  - `keld-guard`, `src/windows_machine/uac_token.rs` (its KEL-270 T4d rule is a closed
    list, which allows `OpenProcessToken(TOKEN_QUERY)`, `OpenThreadToken(TOKEN_QUERY)` and
    `GetTokenInformation(TokenGroups, TokenElevation)` today): add
    `GetTokenInformation(TokenSessionId)` for the helper's own-session check, and
    `GetTokenInformation(TokenPrivileges)` with `LookupPrivilegeValueW` for the
    `SeImpersonatePrivilege` proof, both on the helper's own token;
  - `keld-guard`, new `src/windows_machine/initiating_token.rs`: `OpenProcessToken` with
    `TOKEN_QUERY | TOKEN_DUPLICATE | TOKEN_ASSIGN_PRIMARY` on the host process handle
    that the D2 bootstrap pinned; `DuplicateTokenEx` to a primary token for launch and
    to an impersonation-level token for source pinning; and `SetThreadToken` on the
    current thread with `RevertToSelf` for that token-based impersonation, where a
    failed revert terminates the process;
  - `keld-guard`, new `src/windows_machine/logon_session.rs`: `LsaGetLogonSessionData`
    and `LsaFreeReturnBuffer` (new `windows-sys` feature
    `Win32_Security_Authentication_Identity`);
  - `keld-guard`, new `src/windows_authenticode.rs` (owner decision D4): the KEL-135
    verifier moves here from `keld-core`'s `app_session.rs` with its calls unchanged:
    `WinVerifyTrust`, `WTHelperProvDataFromStateData`, `WTHelperGetProvSignerFromChain`,
    `WTHelperGetProvCertFromChain`, `CryptDecodeObjectEx` and `CryptEncodeObjectEx`.
    `keld-core` loses that allowance and calls the `keld-guard` owner over its existing
    edge, as the helper does; copying the verifier is forbidden. Both AGENTS.md files
    are amended;
  - `keld-guard`, `src/windows_authenticode.rs` (slice S9d; KEL-270 owner decision
    `740998f4-9a47-4527-9e1b-1adb10f4836e`, 2026-10-07, item 1): the running-image
    binding of "Helper launch and self-anchor" adds the source of the running image's NT
    path that S9d selects, `QueryFullProcessImageNameW` with `PROCESS_NAME_NATIVE` on
    the current process or `GetMappedFileNameW` on the module base, and any call that
    its comparison with the opened file object needs. Each is named in `keld-guard`'s
    D4 Authenticode rule by S9d's amendment. `QueryFullProcessImageNameW` needs only
    `Win32_System_Threading`, which `keld-guard` already enables, and has two
    module-private Win32-format call sites today
    (`crates/keld-runtime/src/windows_job.rs:905`,
    `crates/keld-core/src/app_session.rs:1280`); `keld-guard` depends on no `keld-*`
    crate, so its call is not a duplicated shim. `GetMappedFileNameW` and its
    `kernel32.dll` export `K32GetMappedFileNameW` both need
    `Win32_System_ProcessStatus`, which the workspace pin lacks; the first links
    `psapi.dll`, which the helper's import-table allowlist does not admit
    (`crates/keld-updater-helper/tests/release_image.rs:19-26`), unless S9d calls the
    second;
  - `keld-ipc`, `src/windows_named_pipe.rs`: an exact expected-ACE set instead of the
    one current-user ACE (the initiating user's SID for connect-back; the host's own
    SID plus BUILTIN Administrators for the bootstrap), the explicit Medium no-write-up
    label and, for the endpoints the elevated helper creates, the `O:BA` owner at
    creation, with readback of all three; client-side readback of the server
    descriptor's owner, DACL and label; the `is_attempt_endpoint` exact-shape predicate
    beside the landed ones in `bootstrap.rs` (safe code);
    and `TokenStatistics`, `TokenElevation` and `TokenElevationType` added to the public
    `query_windows_peer_token_facts`, which stays the single token-fact reader for the
    claim writer's token and the initiating token alike. `GetNamedPipeServerProcessId`,
    `GetNamedPipeServerSessionId`, `GetNamedPipeClientProcessId`,
    `ImpersonateNamedPipeClient`, `RevertToSelf` and `DisconnectNamedPipe` are already
    listed; `ProcessIdToSessionId` on this process's own ID is added for the client's
    same-session check (S4a).
    The pipe impersonation is used only to read the claim writer's token at
    identification level; it never pins a source. The descriptor code uses
    `Win32_Security_Authorization`, which the workspace pin already enables. The new
    safe module `src/attempt.rs` holds the `keld-attempt` codec, server and client and
    the endpoint locator of "Candidate connect-back" (approved: KEL-270 owner decision
    `eff8e2fb`, 2026-10-06);
  - `keld-runtime`, `src/windows_job.rs` (its KEL-270 whitelist): `CreateProcessWithTokenW`
    with `CREATE_SUSPENDED`; `AssignProcessToJobObject` on that suspended process (S11)
    and on the `PerUserDirect` same-token suspended child (S6b) (a listed call with a
    new scope); `CompareObjectHandles`; the one crate-private clear of
    `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` with `SetInformationJobObject` and a
    `QueryInformationJobObject` read-back of `0` (slice S6b3; "Candidate release after
    commit"), with exactly two scopes: the exact attempt Job through
    `WindowsProcessJob::release_family`, after `complete()` in `PerUserDirect` (S6c)
    and after `health-accepted` in `MachineUacDirect` (S11), and the host-death Job
    through `WindowsHostDeathJob::release_for_exit`, after the census at a committed
    exit (listed calls with a new scope; the amendment also narrows the crate rule that
    forbids Job limit changes to admit exactly this strip); the production census of
    `release_for_exit` (S6b3): `QueryInformationJobObject(JobObjectBasicProcessIdList)`
    on the host-death Job, `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION |
    PROCESS_TERMINATE | PROCESS_SYNCHRONIZE)` on each listed member, `IsProcessInJob`
    against the host-death Job and the attempt Job, and `TerminateProcess` and
    `WaitForSingleObject` on members outside the family (listed calls with new scopes);
    `SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32)`, the System32-only search;
    `ShellExecuteExW` with `runas`, `SEE_MASK_NOCLOSEPROCESS`, `SEE_MASK_NOASYNC` and
    `SEE_MASK_FLAG_NO_UI`, the helper's own directory as `lpDirectory`, `nShow`
    `SW_HIDE` and no owner window, on a dedicated thread with `CoInitializeEx`
    (single-threaded apartment) and `CoUninitialize` around it ("Helper launch and
    self-anchor"; new `windows-sys` features `Win32_UI_Shell` and
    `Win32_System_Com`); and the S6b3 process-ID lister over
    `QueryInformationJobObject(JobObjectBasicProcessIdList)`, production code for the
    host-death census, which S12 reuses for the Machine-UAC census of "Machine-UAC
    owner-loss retirement" instead of adding an acceptance-only seam, never as
    retirement evidence. `OpenProcess`, `GetProcessTimes`,
    `QueryFullProcessImageNameW`, `OpenProcessToken`, `IsProcessInJob`,
    `TerminateProcess` and `WaitForSingleObject` are already listed;
  - reuse decisions in `keld-runtime`: the suspended child's resume-once state and its
    single `ResumeThread` call in `windows_lpac.rs` (`spawn_suspended` at :313,
    `resume` at :564) are generalized into one suspended-child type that the LPAC
    launch, the token launch and the `PerUserDirect` candidate launch use, so no second
    `ResumeThread` call is added; the LPAC
    `spawn_suspended` itself is not reused, because it creates through `CreateProcessW`
    with an LPAC attribute list and the caller's own token. The `PerUserDirect`
    candidate, which runs under its owner's own token, is created through
    `CreateProcessW` with `CREATE_SUSPENDED`, under the caller's own token and without
    the LPAC attribute list, beside the LPAC creation in `windows_lpac.rs` (an existing
    call with a new scope; slice S6b; Coordination record, Linear KEL-270 comment
    `7905ec8a-2529-4c23-90f4-878d315bc0e5`, 2026-10-07). `assign_child`
    (`windows_job.rs:1479`) takes a `std::process::Child`, which cannot represent the
    process that `CreateProcessWithTokenW` or that suspended creation returns, so it is
    extended to accept the suspended child's owned process handle rather than
    duplicated. The attempt-Job
    stdin start gate (`windows_job.rs:2323-2341`) is not reused for the candidate:
    suspended creation gives Job membership before the first instruction without an
    inherited pipe, and handle inheritance through `CreateProcessWithTokenW` is not
    qualified;
- existing doctor/build diagnostics and native fixtures.

Must not touch in Slice A:

- KIPC frames, renderer bridge or permission syntax;
- v0 manifest fields/meaning;
- delta dependencies/algorithms;
- store/package-manager mutation APIs;
- application databases or a migration engine;
- macOS/Linux activation before KEL-137 and native qualification.

## 6. Tasks

- [x] T1 — synchronized this approved multi-mode contract with Architecture 06, regenerated
  included docs, and independently reviewed the exact cross-document head. Direct-mode
  implementation follows this contract; the product decision does not authorize a
  seamless privileged mechanism.
- [ ] T2 — v0 manifest/full verifier plus protected provenance admission/refusal; no
  delta dependency.
- [ ] T3a — produce canonical Windows x64 v0 full packages on a Windows host, with the
  exact no-migration policy owned by `keld-pack` and byte-checked by `keld-update`; add
  independent ustar/policy golden vectors, producer-to-verifier digest/size agreement,
  invalid-name zero-write controls and non-Windows typed refusal. This producer-only
  task does not write extracted files.
- [ ] T3b — after T3a, add two-pass protected Windows extraction beneath the admitted
  staging root; retain guard-owned case/NFC/namespace rejection, hostile archive corpus,
  and real reparse/rename substitution refusal before any write.
- [ ] T4a — KEL-266: actual SYSTEM initializer, protected exact-baseline seeds,
  provenance-last publication, stable activation-lease seeding, read-only loader and
  persistent ancestry proof; no activation.
- [ ] T4b — common Windows x64 direct transaction: journal, floor/current/LKG order,
  attempt-bound 30-second health, one mode-supplied write lease, and crash cut at every
  persisted boundary; no per-mode state machine fork. Progress: the common transaction
  (production journal, fixed-slot write-through replacement, one step function for
  forward progress and recovery, exact receipt/retirement/coordinator binding and
  version retirement) passes PerUserDirect subprocess crash cuts at every persisted
  boundary. The attempt owner's private health channel, 30-second `Ready` observation and
  installed-host QF1 composition remain.
- [ ] T4c — default per-user install/bootstrap and no-UAC authority; prove v2 owner/mode
  provenance, stable lease seeding and hostile-role write denial under the user's LocalAppData tree.
- [ ] T4d — explicit-UAC Program Files authority; prove the installer token can assign
  BUILTIN Administrators as owner (`SE_GROUP_OWNER`, not deny-only), exact protected
  ancestor/state DACLs and canonical descriptors on published records; filtered-token
  denial-zero-write; the D2 bootstrap, exact candidate revalidation, initiating-user
  candidate launch and live-owner health/rollback; after owner death or reboot an
  ordinary launch returns `MachineRecoveryRequired`, and the helper's recovery-only role
  resolves it from a fresh UAC prompt without ordinary host boot, writing nothing when
  revalidation or the process-family proof fails. The §7 rows for criteria 8, 17 and 20
  define the expected results. Implementation follows these dependency-ordered slices,
  each one PR (S4 is two, S4a and S4b; S6 is six, S6a, S6b, S6b2, S6b3, S6c and S6d; S9
  is four, S9a to S9d) with its crates, gates and evidence:
  - S1, native qualification spike, in the nested research checkout only (no Keld
    code; no gate): `hProcess` and its rights from an elevated `runas` launch; the
    elevated helper opening the host process and its token after own-account and
    alternate-administrator consent; a Medium claimant opening the elevated owner
    (else the D5 fallback); same-session `CreateProcessWithTokenW` with
    `SeImpersonatePrivilege`, `CREATE_SUSPENDED` and Job assignment before the first
    instruction; the default label of a pipe an elevated process creates;
    `PIPE_REJECT_REMOTE_CLIENTS` against a local SMB loopback open;
    `STATUS_NO_SUCH_LOGON_SESSION`; and whether a full restart, a sign-out and a Fast
    Startup shutdown end the initiating session. Evidence: research receipts with the
    exact Windows build. Stop rule: if the helper cannot open the host process or its
    token after alternate-administrator consent, T4d stops before S11 for a new owner
    decision ("Machine-UAC bootstrap" item 4).
  - S2, typed `MachineRecoveryRequired(MachineRecoveryGuidance)` in `keld-update`
    (`error.rs`, `windows_baseline/load.rs`). Gates: public API (breaking). Evidence:
    exact-text guidance tests, the §7 recovery-required row, `PerUserDirect` unchanged.
  - S3, journal v2 and the mint-then-journal seam in `keld-update` (`records.rs`,
    `windows_baseline/activate.rs`). Gates: wire, public API. Evidence: v2 golden
    vectors, v1 decoding that admits no claim, unchanged crash cuts.
  - S4, the `keld-attempt` endpoint layer, codec, server and client in `keld-ipc`
    (`attempt.rs`, `bootstrap.rs`, `windows_named_pipe.rs`), in two PRs. Coordination
    record (Linear KEL-270 comment `3972c6dd-1bae-4c29-9c8b-95d621600057`,
    2026-10-06): the record byte layouts wait for the T4d wire review that "Candidate
    connect-back" requires before code, so S4 splits into the parts this specification
    already fixes (S4a) and the parts that wait for that review (S4b).
    - S4a carries no wire-format content: no record, byte layout, codec, golden vector
      or locator. It adds the exact-shape `is_attempt_endpoint` predicate, the three
      closed endpoint descriptor forms, first-instance endpoint creation and descriptor
      readback, the identification-level client, which requires the server's session
      and exact descriptor before it sends anything, and the `TokenStatistics`,
      `TokenElevation` and `TokenElevationType` fields of
      `query_windows_peer_token_facts` (§5); the descriptor readback shares the one
      ACL-entry equality that `keld-guard` (`windows_machine.rs`) exports. Gates: unsafe
      (`keld-ipc` amendment), public API, permission model (the connect-back DACL), wire
      (the `keld-attempt-<64 hex>` namespace only). Evidence: the landed rows for the
      exact shape and cross-namespace negatives, descriptor readback and squatting
      refusal, the client's session check and the token facts.
    - S4b (approved: KEL-270 owner decision `eff8e2fb`, 2026-10-06): the codec for
      the claim and health records with its golden vectors and fuzzing, and the
      purpose-`1` endpoint locator, which shares one crate-private prefix constant with
      the S4a `is_attempt_endpoint` predicate; the bootstrap records and the purpose-`2`
      locator land with S11 (*Messages*, *Locator*). Gates: wire, unsafe (`keld-ipc`
      amendment), public API, dependency (the `keld-ipc` `blake3` edge). Evidence:
      codec goldens and fuzzing, the purpose-`1` locator golden vector and the
      negatives of the §7 "8 (keld-attempt codec)" row.
  - S5, the `CompareObjectHandles` binding in `keld-runtime` (`windows_job.rs`),
    reusing the generalized LPAC suspended-child path. Gates: unsafe (`keld-runtime`
    amendment), public API (breaking rename `WindowsLpacChild`→`WindowsSuspendedChild`;
    new `WindowsLaunchedProcess`, `WindowsClaimantRefusal`). Evidence: the
    process-object cells of the claimant-binding row at the binding; pipe, token,
    deadline and one-shot cells close in S6/S11.
  - S6, `PerUserDirect` connect-back end to end, in six PRs. Coordination record
    (Linear KEL-270 comment `7905ec8a-2529-4c23-90f4-878d315bc0e5`, 2026-10-07): S6
    splits into S6a to S6d; S6a and S6b are independent, S6c composes them, and S6d
    composes the landed lifecycle keeper into S6c's coordinator. Owner decision (Linear
    KEL-270 comment `740998f4-9a47-4527-9e1b-1adb10f4836e`, 2026-10-07, item 3) adds
    S6b2, which changes the coordinator input that S6c composes and lands before it.
    Item 2 of the same decision, under the owner's rule in reply `559c07ac` and the
    failed option-A qualification in reply `079b239d`, adds S6b3, the candidate release
    that S6c composes; it also lands before S6c (coordinator decisions under the
    owner's delegation, 2026-10-08; "Candidate release after commit").
    Until S6d lands, an owner lost at any point after the candidate launch (`AwaitingHealth`,
    `HealthAccepted` or `RollbackPending`) leaves its journal pending; `recover` needs a
    retirement binding that only the keeper's `KELD-QF1` witness supplies once the owner
    is gone, so later startups refuse with `JournalBoundRecoveryRequired` and no
    production path resolves it. No production caller invokes S6c's coordinator before
    S6d. Each slice sets its deadlines from measurements that its PR records.
    - S6a, the `keld-attempt` exchange in `keld-ipc` (`attempt.rs`, `attempt/records.rs`
      and the `keld-ipc` fuzz manifest). Coordination record (Linear KEL-270 comment
      `7905ec8a-2529-4c23-90f4-878d315bc0e5`, 2026-10-07): the claim (`AH1` to `AR1`)
      and the health records run inside `WindowsAttemptEndpoint` and
      `WindowsAttemptClient`, which never expose the stream; the endpoint derives its
      name from the IDs that its `AC1` carries; and the exchange owns the per-connection
      deadline, the refusal and re-arm of the same instance, the one-shot, the caller's
      claimant check, the owner's health-window logic (in this module, with its
      30-second constant and, from S6c, the measured G), `AK1` and the owner's
      end-of-file wait, and the candidate's `AK1` read under a deadline. The S4b record
      constructors, readers and writers become crate-private (KEL-270 comment
      `136c2682`), as does every other attempt item no longer used outside `keld-ipc`,
      and the fuzz entry moves behind a `fuzzing` feature. Its first commit is the
      move-only split of `attempt/tests.rs`. Gates: public API (breaking: the
      narrowing; new: the exchange), permission model (connect-back acceptance and the
      one-shot), wire (the approved transcript and health sequence on the pipe; no new
      bytes); unsafe and dependency: none (the window wait reuses the landed
      deadline-bounded read, and the feature adds no crate). Evidence: the exchange
      cells of the "8 (keld-attempt codec)" row (the locator check before `AA1`, the
      foreign `AH1`, the `AC1` server process ID and the silent non-admitted magic); the
      pipe cells of the claimant-binding row (a silent connector dropped at its
      per-connection deadline, refusals that consume no one-shot and do not extend the
      health deadline, then acceptance on the same instance, a failed `RevertToSelf` and
      the non-inheritable client handle); and, at the pipe, the "one byte after `AY1`"
      cell and the `AK1` disconnect negative control of the "8 (health sequence)" row.
    - S6b, the owner and claimant primitives in `keld-update` and `keld-runtime`.
      Coordination record (Linear KEL-270 comment
      `7905ec8a-2529-4c23-90f4-878d315bc0e5`, 2026-10-07): the claimant's lease-less
      provenance read; criterion 20's authenticated candidate-boot read, with the
      health-receipt digest that the candidate computes for `AB1`; the owner's expected
      digest; the durable `HealthAccepted` step, split from completion so that the owner
      sends `AK1` accepted exactly after the durable write (*Health sequence*); the
      launch handle's liveness check; and the `PerUserDirect` candidate launch, a
      same-token `CREATE_SUSPENDED` launch through the generalized suspended-child type
      with Job membership before the first instruction, which does not reuse the stdin
      start gate (§5). The digest is exposed only through the attempt and candidate-boot
      accessors, and its derivation stays crate-private. S6b and S9b both amend
      `crates/keld-runtime/AGENTS.md`, so they land one after the other, and the second
      carries an explained instruction-budget change. Gates: unsafe (`keld-runtime`
      amendment), public API (breaking: `accept_health` becomes two steps;
      `WindowsProcessJob::assign_child` returns a `WindowsJobMembership` proof, and it,
      `contains_child` and `terminate_and_wait` take `impl Into<WindowsJobProcess>`;
      `WindowsLaunchedProcess::resume` takes that proof, and `record` refuses an
      already-resumed child; `WindowsSuspendedChild::resume` refuses a same-token child,
      which resumes only through its launch record, and refuses a previous suspend count
      other than 1; new: the two reads, the digest accessors, the liveness check, the
      launch, and the `WindowsJobProcess` and `WindowsJobMembership` types), permission
      model (criterion 20's reader exception); dependency and wire: none (it reads the
      landed journal v2). Evidence: each candidate-boot refusal with a typed `WriterActive`;
      the process-object cells of the claimant-binding row at the owner; a subprocess
      crash cut between the durable `HealthAccepted` step and completion (rows 6–7, 9);
      and the candidate's Job membership before its first instruction.
    - S6b2, the verified coordinator image in `keld-update` (`windows_extraction.rs`,
      `windows_baseline/activate.rs`). Owner decision (Linear KEL-270 comment
      `740998f4-9a47-4527-9e1b-1adb10f4836e`, 2026-10-07, item 3): `begin_activation`,
      `recover` and `resume_unlaunched` take the attempt owner's `VerifiedWindowsImage`,
      and `keld-update` derives `helper_image_blake3` from its file with its single
      `image_blake3` owner; the tests reach one crate-private file-taking function, and
      `keld-guard` gains no test constructor (§5). It is a slice of its own rather than part of S6c because
      it changes only `keld-update`, a crate outside S6c's, under the public-API gate
      alone, and because S10 and S11 call the same three entry points, so S6c's crates
      stay `keld-core`, `keld-host` and the `keld-ipc` attempt module. It lands before S6c
      and waits for no unlanded slice. Gates: public API (breaking: the three signatures);
      unsafe, permission model, dependency and wire: none (the `keld-update ->
      keld-guard` edge exists, and the journaled digest's derivation and bytes are
      unchanged). Evidence: the "10, 17 (coordinator image)" row; the landed
      activation, recovery and crash-cut tests pass through the crate-private
      file-taking function in place of each raw digest.
    - S6b3, the candidate release primitives in `keld-runtime` (`windows_job.rs`), with
      the mechanical call-site updates in `keld-host` (§5). Owner decision (Linear
      KEL-270 comment `740998f4-9a47-4527-9e1b-1adb10f4836e`, 2026-10-07, item 2, under
      replies `559c07ac` and `079b239d`; coordinator decisions under the owner's
      delegation, 2026-10-08; "Candidate release after commit"): the retained
      `WindowsHostDeathJob` capability that `install_host_death_job` returns; the one
      clear-with-read-back primitive with its two consuming entries,
      `WindowsProcessJob::release_family` and `WindowsHostDeathJob::release_for_exit`;
      the host-death Job process-ID lister; and the census, with its termination of
      members outside the family and its deadline refusal. It lands before S6c, which
      composes it in the §4 order and measures the census deadline; S11 reuses
      `release_family`; S12 reuses the lister. It changes crash ownership and handle
      ownership, so it needs an architecture review. Gates: unsafe (`keld-runtime`
      amendment: the §5 scopes on listed calls, no new function names, and the
      limit-change rule narrowed to this strip; the amendment carries an explained
      instruction-budget change for `crates/keld-runtime/AGENTS.md` under
      `.agents/instructions.md`, with named independent review evidence as the S6b
      and S9b rule above provides, because the file is at 1833 bytes against its
      1856-byte cap in `.agents/instruction-budget.tsv`), public API (breaking:
      `install_host_death_job`'s return type; new: `WindowsHostDeathJob`,
      `release_for_exit`, `release_family`, `WindowsReleasedAttempt` and
      `WindowsExitCensus`); permission model, dependency and wire: none (every call
      and constant is under a `windows-sys` feature the crate already enables, and
      the clear removes only a termination the host could already perform on its own
      family). Evidence: the "8, 9 (candidate release)" row except its S6c cells, in
      child processes that stand in for the host and the candidate.
    - S6c, the composition in `keld-core` and `keld-host`, and the measured G constant
      in the `keld-ipc` attempt module. Coordination record (Linear KEL-270 comment
      `7905ec8a-2529-4c23-90f4-878d315bc0e5`, 2026-10-07): the `PerUserDirect` host
      coordinator, which passes S6b2's entry points the `VerifiedWindowsImage`
      from the host's one `keld-guard` verification of its own image (KEL-270 owner decision `740998f4`,
      item 3); the `app_session.rs` candidate mode that defers both the
      recovery-gate arm and the terminal-revocation predicate to `AK1` accepted
      (*Health sequence*); `keld-host`'s rendezvous argument; and the measurement that
      fixes the margin G (approved: KEL-270 owner decision `eff8e2fb`, 2026-10-06),
      whose constant joins the 30-second one and whose artifact is kept in the
      keld-benches repository, with the harness in this repository. Gates: public API
      (the coordinator entry and the `keld-host` argument
      contract), permission model (claimant admission end to end); unsafe, dependency
      and wire: none. Evidence: row 8; the `PerUserDirect` host-coordinator cells of the
      connect-back, claimant-binding and endpoint-squatting rows; the "17 (argument
      shape)" row for the candidate's rendezvous argument; row 20's typed `WriterActive`
      for every launch during an attempt that is not the accepted claimant; and the "8
      (health sequence)" row with the G measurement, except its owner-killed cell (S6d).
      S6c keeps the host's boot `VerifiedWindowsImage` for that call; today
      `validate_installed_current_exe` drops it on return
      (`crates/keld-core/src/app_session.rs:2360-2372`). S6c composes S6b3's release
      in the order of "Candidate release after commit": after `complete()` returned
      `Ok`, `release_family` (on `Err` it exits with both Jobs still kill-on-close);
      then the host's own role teardown, the landed WebView2 `BrowserProcessExited`
      barrier and the Bun teardown; then `release_for_exit`, with the census deadline
      that S6c measures from the reaping latencies its PR records and fixes beside G;
      then exit. It keeps `install_host_death_job` first on the rendezvous-argument
      path, so the candidate's own host-death Job nests under the attempt Job. Its
      candidate mode never opens the WebView2 user data folder that the old host
      holds, the assumption under which the census may terminate a host WebView2
      process past the barrier (§4 *Census and policy*). S6c also owns a design item
      for the candidate's WebView2 profile during health: landed release boot selects
      its persistent user-data folder before any listener, child or window
      (Architecture 05 §1, `docs/architecture/05-webview-and-native.md:55-56`), and
      the KEL-135 lease excludes a second same-app host until the host owner exits
      (`05-webview-and-native.md:88`), so a candidate launched as landed would fail
      with `ProfileInUse` while the old host holds the lease and could never reach
      `Ready`; S6c must specify a profile handover, or a candidate-only profile, before
      it starts, with no temporary-store fallback (`05-webview-and-native.md:64-65`).
      Its evidence adds the S6c cells of the "8, 9 (candidate release)" row: survival
      through the real host coordinator, the ten in-session updates, the measured
      deadline and the `complete()` `Err` cut (r12).
    - S6d, the `PerUserDirect` owner-loss composition in `keld-core` and the keeper's
      executable entry. Coordination record (Linear KEL-270 comment
      `7905ec8a-2529-4c23-90f4-878d315bc0e5`, 2026-10-07): S6c's coordinator starts a
      dedicated one-shot keeper process outside the attempt Job and hands it the Job and
      lease retention through the landed `KELD-HO1`/`KELD-HR1` handoff; the next writer,
      as successor, takes the `KELD-QO1`/`KELD-QA1`/`KELD-QF1` witness before `recover`.
      S6d names the keeper's executable entry and its crates; none exists today.
      Design item, which S6d's own specification solves at the root before S6d starts
      (§10): a keeper that the host starts inherits the host's host-death Job and is
      outside the attempt Job, so option B does not cover it, and the S6b3 census would
      terminate it as a member outside the family at a committed exit; it has no
      release path today ("Candidate release after commit"). A criterion-10 post-exit
      helper, if one is used, has the same gap. The
      adversarial controls of "Bounded per-attempt lifecycle keeper" pass before S6d
      connects the keeper to production writes. After S6d, recovery when every owner is
      lost, or across a reboot or hibernation, remains unsupported and halts with its
      evidence preserved ("Bounded per-attempt lifecycle keeper"). Gates: permission
      model (owner-loss retirement); public API if the coordinator entry changes or a
      keeper entry is added; unsafe, dependency and wire: none unless the keeper entry
      adds an FFI call or a crate edge, which S6d then gates. Evidence: the "8 (health
      sequence)" cell in which an owner killed between the end of the window and the
      durable `HealthAccepted` has sent no `AK1` and recovery rolls back.
  - S7, `keld-guard` token, logon-session and token-impersonation wrappers
    (`uac_token.rs`, `initiating_token.rs`, `logon_session.rs`). Gates: unsafe
    (`keld-guard` amendment), dependency (`Win32_Security_Authentication_Identity`),
    public API (new safe wrappers and error types exported by `keld-guard`).
    Evidence: wrapper tests, including the seam-injected session statuses.
  - S8, the KEL-135 verifier move to `keld-guard` (`windows_authenticode.rs`) with
    `keld-core` calling it. Owner decision (Linear KEL-270 comment
    `4f5ce05b-8a79-4288-9a88-59d79d45e3f5`, 2026-10-06): S8 lands ahead of KEL-19's
    writer and container and KEL-254 T3 Part B. Gates: unsafe (`keld-core` and
    `keld-guard` amendments), public API. Evidence: the existing KEL-135 rows pass
    unchanged against the moved owner, plus a before/after signed-image receipt. The
    payload read-back moves to its producers' slices: from `keld-host.exe` to KEL-254
    T3 Part B and from `keld-updater-helper.exe` to S9. T3 Part B reads the payload
    through `VerifiedWindowsImage::file`, the handle that verification pinned.
  - S9, the updater helper, in four PRs. Coordination record (Linear KEL-270 comment
    `7905ec8a-2529-4c23-90f4-878d315bc0e5`, 2026-10-07): S9 splits into S9a to S9c; S9a
    and S9b are independent, and S9c composes them. Owner decision (Linear KEL-270
    comment `740998f4-9a47-4527-9e1b-1adb10f4836e`, 2026-10-07, item 1) adds S9d after
    S9c. The recovery role stays disabled
    (`RecoveryDisabled`), and until S11 the activation role refuses after the
    self-anchor (*Interim*).
    - S9a, in `keld-pack`, `keld-update` and `keld-core`. Coordination record (Linear
      KEL-270 comment `7905ec8a-2529-4c23-90f4-878d315bc0e5`, 2026-10-07): the
      `keld-pack` helper member, which `keld-update`'s archive check consumes after the
      content digest; the `locate.rs` image choice that replaces the fixed `HOST`
      constant; the helper's self-anchor entry point in `keld-update`; and the
      activation role's helper-image derivation from the selected tree. It also moves the
      rule that the verified signer's publisher scope and app id equal the recorded ones
      from `keld-core` (`require_recorded`, `app_session.rs:2292-2313`, including the
      app-id check at `:2303`) into `keld-update`, so that one owner serves both images,
      and the host's refusal moves from `KELD-WV-009` to a `keld-update` code in S9a's
      reserved range (§5). It generalizes the KEL-19 container's error texts to name the
      failing image, adding no variant, code path or error code (KEL-19 container spec
      §1). Gates: wire (canonical package content), public API (the locator's image
      choice, the self-anchor and derivation entry points and the moved signer rule);
      unsafe, permission model and dependency: none. Evidence: the canonical-content
      goldens and the helper-member cell of rows 5, 13; the self-anchor and
      host-derivation cells of the "17 (helper launch and self-anchor)" row; the landed
      executable-located rows pass unchanged for `keld-host.exe`; cross-image locator
      refusals; and KEL-254 AC11's publisher and app-id negative controls pass against
      the moved rule.
    - S9b, the `keld-runtime` launch calls (`windows_job.rs`). Coordination record
      (Linear KEL-270 comment `7905ec8a-2529-4c23-90f4-878d315bc0e5`, 2026-10-07):
      `SetDefaultDllDirectories`; `ShellExecuteExW` with `CoInitializeEx` and
      `CoUninitialize` around it; and the `--recovery-role` selector constant ("Helper
      launch and self-anchor"). It and S6b land one after the other (see S6b). Gates:
      unsafe (`keld-runtime` amendment), dependency (`Win32_UI_Shell` and
      `Win32_System_Com`), public API (the safe wrappers and the selector constant),
      wire (the selector literal, an argument contract between a host and a helper of
      another version tree); permission model: none. Evidence: the post-`main` loader
      cell of the "17 (helper launch and self-anchor)" row, run in a child process; the
      launch wrapper's refusals before any call; and the operator consent and decline
      cells (`hProcess` is the exact elevated image; a decline is typed with zero
      protected writes). The launch has no production caller until S11.
    - S9c, the `keld-updater-helper` crate. Coordination record (Linear KEL-270 comment
      `7905ec8a-2529-4c23-90f4-878d315bc0e5`, 2026-10-07): it starts with the
      static-runtime link spike, and if the per-binary link fails it stops for an owner
      decision; then the crate with its loader hardening and static runtime, built for
      the `windows` subsystem; the helper's argument check, which calls the S4a
      `is_attempt_endpoint` predicate and itself parses only the fixed recovery-role
      selector (approved: KEL-270 owner decision `eff8e2fb`, 2026-10-06), whose literal
      it imports from S9b; and the role dispatch with both interim refusals. Gates:
      dependency (new crate), public API (the helper's argument contract); unsafe,
      permission model and wire: none. Evidence: the "17 (argument shape)" row for the
      helper's argument; the pre-`main` planted-DLL, import-table and edge-set cells of
      the "17 (helper launch and self-anchor)" row; the helper's recovery-role refusal in
      the "17 (recovery-required state)" row; and the activation role's pinned interim
      refusal.
    - S9d, the running-image binding in `keld-guard` (`windows_authenticode.rs`) and its
      two callers, the installed host's boot in `keld-core` (`app_session.rs`) and the
      helper's self-anchor in `keld-updater-helper` (`helper.rs`). Owner decision
      (Linear KEL-270 comment `740998f4-9a47-4527-9e1b-1adb10f4836e`, 2026-10-07, item
      1, on finding `c4921888`): the single `keld-guard` verifier binds the file that it
      opens and verifies to the running image ("Helper launch and self-anchor",
      *Running-image binding*). It lands after S9c, whose helper is one of its callers,
      and before S10 and S11, which add the helper's first write path. Its prerequisite
      is the research spike that paragraph names, run in the nested research checkout
      with no Keld code, and S9d cites the spike's receipts. If they show that neither
      source names the running image's file, that the junction-retarget negative control
      cannot be built, or that the binding would refuse a launch whose running image is the
      located tree's verified file and that the path-only open admits today, S9d stops
      for an owner decision. Gates: unsafe
      (`keld-guard` amendment of its D4 Authenticode rule, §5), public API (the
      verifier's running-image entry and its typed refusal, and both callers' switch to
      it), permission model (the helper's self-anchor and the installed boot admit only
      the running image); dependency: any `windows-sys` feature that S9d's selected source or
      comparison adds, such as `Win32_System_ProcessStatus`, which gates both
      `GetMappedFileNameW` and `K32GetMappedFileNameW` (§5); wire: none. Evidence: the "17
      (running-image binding)" row; the landed KEL-135 rows, the leaf-reparse refusal
      and the "17 (helper launch and self-anchor)" cells pass unchanged.
  - S10, the recovery-only role under D1 (refined): the abandon intent in
    `next_activation_step` with the five step-mapping changes and the `retirement_due`
    change listed in §5, and its public recovery-role entry point (`keld-update`), the
    role in `keld-updater-helper`, and
    Machine-UAC admission in `load_windows_recovery_inspection`. Gates: public API,
    permission model. Evidence: the `publish-pending` owner-loss, abandon-intent
    crash-cut and recovery-required rows.
  - S11, the activation role with the D2 bootstrap (`keld-updater-helper`, the host
    side in `keld-core`, Machine-UAC admission in `load_windows_activation_write_snapshot`
    reusing `require_windows_machine_uac_owner_token`, `keld-ipc` with the bootstrap
    records `BH1` to `BO1` and the purpose-`2` locator with its golden vector, whose
    source-locator encoding, `BQ1` field set and `BO1` classes its wire review fixes
    under *Bootstrap records*; approved: KEL-270 owner decision `eff8e2fb`,
    2026-10-06), and the `keld-runtime` token launch: `CreateProcessWithTokenW` with
    `CREATE_SUSPENDED`, the token-launch Job assignment and, after `health-accepted`,
    the attempt Job's release through S6b3's `WindowsProcessJob::release_family`, the
    one clear primitive (§5; Coordination record, Linear KEL-270 comment
    `7905ec8a-2529-4c23-90f4-878d315bc0e5`, 2026-10-07; "Candidate release after
    commit"). S11 removes the activation role's interim refusal (*Interim*). Gates: all
    five. Evidence: the bootstrap rows under the UAC operator evidence protocol,
    including the second ordinary account and alternate-administrator rows, and the
    "17 (Machine-UAC Job inheritance)" row. It starts only after S1 showed that the
    helper can open the host process and its token after alternate-administrator
    consent, or after the owner decided otherwise. Start condition (coordinator
    decision under the owner's delegation, 2026-10-08): its §7 rows record the elevated
    helper's and the candidate's membership of the initiating host's host-death Job and
    of the helper's attempt Job, and whether the candidate survives the initiating
    host's exit; if the candidate is in that host-death Job, S11 stops for an owner
    decision ("Machine-UAC owner-loss retirement"). This amendment specifies no probe.
  - S12, enabling owner-loss retirement (`keld-updater-helper`, `keld-guard`'s logon
    wrapper, and the Machine-UAC census over `keld-runtime`'s S6b3 process-ID lister,
    which S12 reuses rather than adding a seam). Gates: permission model, unsafe review
    of the exact diff. Evidence: the owner-loss retirement row with its census and
    static scan; only then does `RestartFirst` replace `RecoveryDisabled` for launched
    attempts.
  T4d also updates the `activate.rs` retirement-binding documentation to name the
  recovery-only helper as a producer. FFI wrappers stay minimal beneath safe typed
  wrappers with owned handles and minimum access rights; no raw handle crosses a normal
  public API; every error path closes handles and reverts impersonation. Failure-first
  and mutation tests cover impersonation, token substitution, wrong session, wrong image,
  stale or replayed attempts, and handle or provenance substitution. Independent unsafe,
  privilege/security and wire reviews and real Windows UAC acceptance are required. If
  same-session `CreateProcessWithTokenW` cannot be proved, `CreateProcessAsUserW` is
  qualified separately and never used as a silent fallback.
- [ ] T4e — machine-seamless product row remains gated: prove exact host/install/attempt
  auth, replay resistance, writer/read-pin handoff, family lifecycle, ordinary candidate,
  exact health and crash recovery before selecting or implementing any native mechanism.
- [ ] T5 — managed-owner refusal, hostile-role denial, locked file/disk/interference/
  concurrency and next-attempt recovery.
- [ ] T6 — after KEL-137, repeat independently for each macOS/Linux format/channel.
- [ ] T7 — separately approve signed data compatibility/migration before a migrating
  release can use automatic binary rollback.
- [ ] T8 — only after the baseline, measure optional delta reconstruction while retaining
  the full fallback.

## 7. Test plan

| Criteria | Proof and falsifier |
|---|---|
| 1, 11–12, 16 | provenance/mode/channel/profile/ACL table and installer seed crash cuts; mode/path/owner substitution refuses before feed/write; per-user installer uses LocalAppData with no UAC and actual hostile-role write denials |
| 2–4 | signed v0 fixtures, duplicate-member parser, equal-precedence build-metadata release pair, floor selection including equal-precedence/different-metadata and below-baseline replay, numeric mutations (`0`, `-1`, fraction, exponent, `2^53 - 1`, `2^53`), shorter/exact/longer compressed and decompressed byte counts, digest boundaries and complete ustar golden bytes; selecting a present delta fails Slice A |
| 5, 13 | independent canonical Windows tar/policy goldens; producer-to-verifier size/hash agreement; missing/duplicate/changed policy refusal; a Windows package without exactly one root regular-file `keld-updater-helper.exe` (absent, a directory of that name, a case variant, a nested copy) refuses before output, and the verifier decides it after the content digest (T4d S9a); link/special/mode mismatch, omitted/duplicate parent directory, separator/ADS/device/forbidden/control/trailing-dot/NFC/case/8.3 aliases and ancestor collisions reject before output; T3b separately tests extraction-order and filesystem reparse/rename substitution |
| 6–7, 9 | state trace and subprocess crash after every durable step, including current published before phase advance; floor above candidate, non-prior intermediate floor, orphan no-journal current and mixed rollback context halt; live/unknown coordinator blocks recovery; corrupt/replay/mix every journal field |
| 10, 17 (coordinator image) | (KEL-270 owner decision `740998f4`, item 3; S6b2) `begin_activation` journals as `helper_image_blake3` the BLAKE3 of every byte of the verified image's file, equal to a digest that the test computes independently from that file's bytes; `recover` and `resume_unlaunched` accept the journaled image and refuse an image whose file bytes hash differently from it before any write, with the journal unchanged; the transaction tests drive the crate-private file-taking function, one operator cell passes a real `VerifiedWindowsImage` of a signed build through each public entry point, and a `compile_fail` doctest shows that a bare `&std::fs::File` does not satisfy them; a negative control that journals a constant in place of the derived digest fails this row; the landed crash cuts pass unchanged |
| 8 | live-coordinator candidate boot skips writer-lock recovery; stale attempt/artifact, coordinator death, early exit, crash, timeout and generic marker fail; exact Ready plus 30 monotonic seconds passes |
| 8 (claimant binding) | only the exact launched and retained process is accepted. Two separate observables cover a copy of the candidate image started during `AwaitingHealth`: a same-user Medium copy, like a second instance from the candidate tree that connects first, opens the endpoint, is refused by `CompareObjectHandles`, is disconnected and refuses with a typed `WriterActive`, after which the same pipe instance accepts the real candidate; an LPAC copy that a hostile role starts is denied at pipe open by the DACL and the label and never reaches `CompareObjectHandles`. A peer whose process ID equals the launched one but whose process object differs (seam-injected), a signaled launch handle, a wrong creation time, and a wrong TokenUser, `AuthenticationId`, integrity or elevation each refuse, as does a token from another session that otherwise matches (administrator-constructed); a connector that sends nothing is dropped at its per-connection deadline; refusals consume no one-shot and do not extend the health deadline; a failed `RevertToSelf` terminates the owner (seam-injected); the candidate's connected handle is non-inheritable and in no role's handle list |
| 8 (endpoint squatting) | at every durable step a test reader of the journal finds the named endpoint already held; a name that exists when a fresh owner creates its endpoint (seam-injected, since the order makes it otherwise unobservable) refuses with `ProtectedStateUnchanged` and no protected write, and for a resumed owner leaves the journal unchanged; a second creation of a live owner's name fails; after owner death, a squatter that creates the name as the same user and one that creates it as a second ordinary user, including one whose process ID equals the journaled owner's (seam-injected), are refused by the claimant on descriptor owner, DACL, label, session or image before it sends anything, or on the journaled owner fields after acceptance; in `MachineUacDirect` a Medium process cannot create the endpoint with an `O:BA` owner; a squatting server receives only an identification-level token; descriptor readback rejects an extra ACE, `FILE_CREATE_PIPE_INSTANCE`, `WRITE_DAC`, `WRITE_OWNER`, a missing Medium no-write-up label, a wrong owner and remote-client admission; the observed default label of an unlabelled pipe that an elevated process creates is recorded; a v1 journal admits no claim |
| 8 (connect-back in every direct mode) | separately for `PerUserDirect` with the host coordinator as owner, `PerUserDirect` with a criterion-10 post-exit helper if one is used, and `MachineUacDirect` with `keld-updater-helper.exe` as owner: the candidate inherits no endpoint, receives only its rendezvous name, connects back, is accepted, reports Ready and commits after 30 seconds, and the claimant-binding and endpoint-squatting rows pass in that cell; no `MachineUacDirect` cell uses a post-exit helper; `MachineSeamlessDirect` is refusal-only, so no claim is accepted before its authority is selected; a pass in one cell does not close another |
| 8 (keld-attempt codec) | (approved: KEL-270 owner decision `eff8e2fb`, 2026-10-06) each S4 record's golden bytes, and one negative per byte rule, each refused: each out-of-set purpose (with S11), class or result byte, including `0`; each magic at a position where it is not admitted; a truncated record and a record with one extra trailing byte; a one-field mutation of `AA1`, of `AR1` and of `AB1`; an `AC1` whose IDs fail the locator check, refused before `AA1`; an `AH1` with a foreign installation ID or client PID; locator calls with an all-zero or a duplicated input; an `AC1` whose server PID differs from `GetNamedPipeServerProcessId`; an `AF1` whose class is not admitted at its position (class `1` after `AB1`, class `2` before it); and a non-admitted magic followed by no further byte, refused before its deadline |
| 8 (health sequence) | (approved: KEL-270 owner decision `eff8e2fb`, 2026-10-06) in candidate mode an unexpected application-generation exit after `AY1` and before `AK1` ends the candidate host with no successor generation, and the owner rolls back; a negative control that arms the recovery gate at `Ready` instead fails this row, and a second negative control that defers the arm but keeps the `Ready`-keyed revocation predicate also fails this row; one byte after `AY1` fails health; an exit injected in the last G of the window fails health; an `AK1` accepted that the candidate loses to end of file, a read failure or its deadline leaves its gate unarmed, so a later generation exit ends the host; after `AK1` accepted the same exit is replaced in-process; an owner killed between the end of the window and the durable `HealthAccepted` has sent no `AK1`, and recovery rolls back; on rollback, `AK1` rolled back is written before the candidate family ends, and the rollback completes when the candidate never reads it; the owner disconnects only after the candidate's end of file or its deadline; a negative control that disconnects right after writing `AK1` accepted leaves the candidate unarmed |
| 8, 9 (candidate release) | ("Candidate release after commit"; S6b3, in child processes that stand in for the host and the candidate; the S6c cells are named) r1, survival: a candidate committed through the §4 order is alive, with an unsignaled handle, after the old host has exited; both Jobs read back `0` before that exit; the next launch of the installation selects the committed version with no journal (S6c cell: through the real host coordinator); r2, one negative control per single clear: with only the attempt Job cleared, with only the host-death Job cleared, and with neither, the same candidate's handle is signaled after the host exits; r3, rollback: no clear runs, both Jobs read back `0x2000` until the host exits (the test reads the attempt Job through the landed `WindowsProcessJob::duplicate_lifecycle_keeper_handle` duplicate, `crates/keld-runtime/src/windows_job.rs:1611`, a non-inheritable handle with the `JOB_OBJECT_QUERY` and `JOB_OBJECT_TERMINATE` rights, taken while the Job is live and before `terminate_and_wait` consumed it; the duplicate is a separate handle to the same Job object and survives that call), and the candidate family is gone when the host's handle is signaled; r4, crash cuts, the host killed at W0a, W0b, W1, W2 and W4: at W0a and W0b the candidate is gone, the journal reads `HealthAccepted` and recovery refuses with `JournalBoundRecoveryRequired` before S6d; at W1, W2 and W4 the candidate is gone, no journal exists and the next launch selects the committed version; after W3 the candidate survives; r5, a straggler held past the deadline (seam-injected: a member outside the family whose handle stays unsignaled after termination, as the `pending_io_family` fixture holds one): `release_for_exit` returns the typed refusal, the host-death Job reads back `0x2000`, the host exits, the candidate is gone, and the next launch selects the committed version (S6c cell: the measured deadline); r6, a Bun descendant parked after the Bun primary exited, in the host-death Job and not in the attempt Job, is terminated by the census through its own handle, that handle is signaled before the clear, the witness counts it, and the candidate survives; r7, negative control without the census: a build that clears the host-death Job without it leaves that parked descendant alive after the host exits, so the row detects the gap; r8, negative control that releases before `complete()`: a build that calls `release_family` first and whose host is then killed before journal removal leaves a `HealthAccepted` journal beside a running candidate, and recovery refuses while it runs, so the row detects the gap; r9, KEL-78/T3 regressions: the landed `windows_host_death_job.rs` rows pass unchanged against the capability; dropping or forgetting the capability never closes the handle, so the Job still reads back `0x2000` and an abnormal host death still reaps the enrolled tree; a `compile_fail` doctest shows that the capability yields no raw handle and is not `Clone`; r10, the released candidate's own host-death Job: it is installed and nested (`nested_under_existing_job`), and the candidate's abnormal death reaps its own parked descendant after the old host is gone; r11, an outer Job and ten updates: a host bounded by an outer kill-on-close Job that the test holds releases its candidate, which still ends when the test closes that outer Job; ten in-session updates in a row, each released, with every assignment succeeding and the last candidate alive (S6c cell: through the real host coordinator); r12, a `complete()` that returns `Err` (seam-injected failed durable step): `release_family` is never called, both Jobs read back `0x2000` until the host exits, the candidate is gone with it, and the journal still reads `HealthAccepted`, as at W0b (S6c cell: through the real host coordinator) |
| 8, 17 (D5 fallback) | when the Medium claimant cannot open the elevated owner, the claim binds on the `O:BA` owner and the session before sending and on the journaled owner process ID after acceptance; when the open is admitted, creation time and `helper_image_blake3` are checked as well |
| 17 (D2 bootstrap) | the host creates the bootstrap endpoint with its two-SID DACL and its own user SID as owner before `ShellExecuteExW`, refuses when no `hProcess` is returned, accepts only a client whose process ID equals the `hProcess` process ID while `hProcess` is unsignaled, and impersonates no one; the helper opens with identification-level QoS and, before sending, verifies that the host process image is its installation's selected `keld-host.exe`, the session, and a descriptor owned by that host's user SID; it takes the initiating token only from that host process object and refuses an elevated or non-Medium initiating token before any protected write; a forged rendezvous argument, a squatting server that is not that `keld-host.exe` (same-user code is outside the boundary and is not claimed), a client that is not the launched helper and a second ordinary user's process each refuse; alternate-administrator consent is admitted with the initiating token unchanged once S1 shows the open works, and otherwise refuses with a typed `ProtectedStateUnchanged` before any protected write; source pinning runs under token impersonation, and a failed revert terminates the helper (seam-injected) |
| 17 (argument shape) | the helper's single argument and the candidate's rendezvous argument are each refused before any open or write when they are a UNC or remote path, a `\\?\` path, another `keld-*` namespace, uppercase hex, a wrong length, or come with any extra argument; only the exact local `\\.\pipe\keld-attempt-<64 lowercase hex>` shape, or the helper's fixed recovery-role selector, is accepted |
| 17 (owner loss in `publish-pending`) | the owner is killed after the `publish-pending` journal is durable and before launch, at each persisted step (before publication, after publication, after the floor, after `current`): no candidate family exists and no logon-session proof is needed; in `MachineUacDirect` an ordinary launch returns `MachineRecoveryRequired` with `RecoverNow` once S10 is enabled, and the recovery role resolves the attempt with the abandon intent of D1 (refined): with the floor at its prior value the published candidate is retired, the journal is removed as `Abandoned`, the floor never moves and the same signed version is retryable; with the floor at the candidate, `current` is restored and the candidate retired before journal removal, and the version is consumed; in `PerUserDirect` the landed lease-only resume is unchanged |
| 17 (abandon-intent crash cuts) | the recovery role is killed at every step of the abandon intent: after its re-mint record, after the candidate's retirement and before journal removal, and, at the candidate floor, after `current` is restored and after the retirement; at each of these cuts the journal is still `PublishPending`, the next ordinary launch returns `MachineRecoveryRequired` with `RecoverNow`, and the next recovery-role run resumes it on the writer lease alone, re-mints again, never needs the logon-session proof and never writes `AwaitingHealth` or `RollbackPending`. A cut after the journal's write-through rename to its `pending-*` leaf leaves no journal: the next ordinary launch selects the rollback target normally, and the next writer removes the `pending-*` leftover. Under the ordinary intent every step mapping and `retirement_due` result is unchanged, including the `CandidateUnpublished` refusal at the candidate floor |
| 17 (owner-loss retirement) | helper terminated and crashed mid-health; helper crash after `health-accepted` both before the limit is cleared (the application ends) and after it (the healthy application keeps running); a `rollback-pending` crash; a live owner still holding the lease; sign-out, a real full restart and a Fast Startup shutdown mid-health, each binary: afterwards the session query returns no such logon session or a session with a different logon time, otherwise the row fails and recovery halts; logon-session LUID reuse with a different logon time (seam-injected); a denied or unknown session-query status (seam-injected), with the single accepted "no such logon session" status confirmed on real Windows; a reference to a token of that session kept alive after sign-out, such as a duplicated token handle in an unrelated process, which keeps recovery halted until it closes; the census at Ready, at health acceptance and before the limit clear, through the `keld-runtime` process-ID lister (S6b3, §5) and per-process `TokenStatistics`; the static scan of the host and WebView crates; negative controls showing that no admitted family member starts a candidate-tree image or family-supplied code outside the attempt Job or under another logon session, and that sandboxed roles cannot (netonly `CreateProcessWithLogonW`, `runas` elevation, and a COM server or scheduled task registered to a candidate-tree image as falsifiers); Job membership before the first instruction under `CreateProcessWithTokenW` with `CREATE_SUSPENDED`; a descendant breakaway attempt; nested KEL-96 host Jobs; and a family still terminating with pending I/O (the `pending_io_family` fixture, seam-injected: an unsignaled handle holds a not-yet-exited member of the attempt Job, which the S6b3 lister then reports; the same device as r5 of the "8, 9 (candidate release)" row). Each row either proves both facts or halts fail-closed, and no family member runs or has pending I/O after a passed proof |
| 17 (helper crash after `health-accepted`) | after the limit is cleared and with the application still running, a helper crash leaves `health-accepted` durable: the next ordinary launch, including a second launch of the application, refuses with `MachineRecoveryRequired` and `RestartFirst` guidance (`RecoveryDisabled` before S12); nothing is written; the recovery role halts while the initiating session is live and finishes the commit only after the session query proves that it ended |
| 17 (Machine-UAC Job inheritance) | (S11 start condition; "Machine-UAC owner-loss retirement") under the UAC operator evidence protocol, S11 records `IsProcessInJob` of the elevated helper and of its candidate against the initiating host's host-death Job and against the helper's attempt Job, and whether the candidate is alive after the initiating host exits following `health-accepted`; a candidate inside the initiating host's host-death Job stops S11 for an owner decision, because the defect that "Candidate release after commit" removes for `PerUserDirect` would recur in `MachineUacDirect` and the helper cannot classify the host's family; a candidate outside it passes; this amendment specifies no probe, and the row is evidence, not a mechanism |
| 17 (recovery-required state) | in `MachineUacDirect`, an ordinary startup that takes the snapshot lease and finds any pending journal phase, or no journal with an absent or undecodable `current` and valid last-known-good, returns `UpdateError::Activation` with `MachineRecoveryRequired` (not `JournalBoundRecoveryRequired`, an `UpdateError::Baseline` refusal or merely no selection) and writes nothing; each `MachineRecoveryGuidance` variant's fix-guidance text matches its pinned bytes; the same states in `PerUserDirect` keep their landed recovery and repair; `MachineSeamlessDirect` keeps its landed `UpdateError::Baseline` refusal; while S10's rows have not passed, the helper refuses the recovery role and the guidance is `RecoveryDisabled`; once enabled, the recovery role repairs `current` only from a last-known-good helper tree and resolves each journal phase by its rule |
| 17 (helper launch and self-anchor) | the host derives the helper path only from admitted provenance and records, and offers nothing without authenticated provenance; a `keld-updater-helper.exe` placed beside the host outside the tree, on `PATH` or in the current directory is never launched; a copy of the helper with planted DLLs in its own directory, one for every imported name and for the C runtime names, launched elevated on a clean VM, loads none of them and refuses on self-anchor; the helper's import table matches its allowlist; a DLL planted in the current directory or in a user-writable `PATH` entry is not loaded after `main`; the edge-set checks fail on an injected `keld-core` edge and on an injected Bun-spawn call; the helper refuses every role before the lease and any write when the recorded mode is `PerUserDirect`, `MachineSeamlessDirect` or managed, when it runs from a copy outside a protected version tree (including a forged tree layout in a user-writable directory), when its signer differs from the publisher scope, when its embedded payload is missing, duplicated or mismatched, or when its image digest is not the journaled one |
| 17 (running-image binding) | (KEL-270 owner decision `740998f4`, item 1; S9d) for the helper and for the installed host separately: a genuine signed image of the same publisher that is not the located tree's file, started through a user-owned junction that is then retargeted at the protected version tree, is refused by the `keld-guard` running-image binding, the helper before its self-anchor, the lease and any write, and the host before any listener, child or window; a negative control that removes the binding lets the same launch pass the self-anchor or the installed boot, so the row detects the gap; a direct launch of the in-tree image passes; a launch whose running image is the located tree's verified file and that the path-only open admits today passes, and if the binding would refuse such a launch, S9d stops for an owner decision (§6) |
| 17 (UAC operator evidence protocol) | every row that needs real UAC consent records the helper's process ID and creation time, before and after snapshots of provenance, floor, records, journal and the version census with descriptors, and the secure-desktop step (consent or credential prompt, and the approving account); evidence counts only under the default prompting policy on the secure desktop, and runs where policy elevates without prompting or UAC is turned off are not admitted as Machine-UAC evidence |
| 10–11, 17 | real Windows locked-file/helper, staged-directory publish and same-volume barrier/read-back crash cuts; elevated installer assigns Administrators owner only when TokenGroups has SE_GROUP_OWNER and not deny-only; exact protected owner/DACL read-back on ancestors and records; filtered medium token and second ordinary user are denied write/create/delete/rename/WRITE_DAC/WRITE_OWNER while read succeeds; SYSTEM/Admin writer controls succeed; at AfterStageCreate/BeforeFileFlush, the same account's filtered medium token cannot create/write/obtain WRITE_DAC on Machine-UAC stage objects; UAC denial, fake host, stale attempt, changed source bytes or fake endpoint cause zero protected publication; over-the-shoulder candidate remains in initiating ordinary token; live helper owns health/rollback; actual admitted Keld roles fail mutations |
| 18 | mechanism-neutral seamless row: wrong host/role/image/token profile/install, fake endpoint, peer exit during acquisition, inherited/duplicated pipe-handle leak, stale/replayed attempt, simultaneous successors, competing writer/read-pin race, live/unknown process family and crash/reboot controls; no task/service chosen without every row passing |
| 19 | trusted MSIX/App Installer/Store/enterprise provenance returns typed defer before network/feed/stage/write; direct updater creates no competing writer |
| 20 | real Windows stable `activation.lock` remains present across release/crash; multiple short read leases coexist and block the writer; exactly one share-zero writer is admitted after readers close; missing/wrong-profile lock refuses; a surviving child cannot be mistaken for a dead process family; candidate closes mutable-record pins and acknowledges bootstrap before app code/health, while immutable selected-tree pins remain held; writer replaces mutable records during candidate health without replacing/deleting pinned immutable trees; while the writer holds the lease, every launch that is not the accepted claimant, including one with a forged rendezvous argument, refuses with a typed `WriterActive` and opens no record; an ordinary user's read handle on a mutable record makes the writer's replacement fail and leaves the journal authoritative (`MachineRecoveryRequired` in `MachineUacDirect`), an availability-only residual with no protected write beyond the journal |
| 14 | deterministic fault injection followed by one successful attempt; delta code absent |
| 15 | future base/patch/reconstructed-content mutations and same-attempt full fallback |

No sleep synchronization. State tests inject transitions; process tests wait on
handles/events with bounded kill switches. Windows evidence records filesystem, build,
source SHA, package/signature identity and raw crash cuts. Other OS results are separate.

## 8. Review gates triggered

- unsafe: yes for T4d — the production FFI calls listed in §5 in `keld-runtime`,
  `keld-guard` and `keld-ipc`, each behind its owner's issue-scoped AGENTS.md amendment
  (an amendment of `keld-guard`'s closed list), and the D4 move, which amends both
  `keld-core`'s AGENTS.md (removing its KEL-135 Authenticode allowance) and
  `keld-guard`'s (adding it); each needs independent unsafe review of the exact diff.
  S6b3 amends `keld-runtime`'s rule with the clear and census scopes of §5 on listed
  calls, adds no function name, narrows the rule that forbids Job limit changes to the
  exact kill-on-close strip with read-back, and retains the host-death Job handle in
  `ManuallyDrop` instead of forgetting it (KEL-270 owner decision `740998f4`, item 2;
  coordinator decisions under the owner's delegation, 2026-10-08); that amendment
  carries an explained instruction-budget change for `crates/keld-runtime/AGENTS.md`
  under `.agents/instructions.md`, with named independent review evidence as for S6b
  and S9b, since the file is at 1833 bytes against its 1856-byte cap in
  `.agents/instruction-budget.tsv`.
  The helper crate holds none. Elsewhere, conditional on each exact native/helper
  implementation;
- public API: yes — canonical package contents, update admission and unsupported-cell
  diagnostics are author-facing contracts. T4d adds: the breaking
  `ActivationEffect::MachineRecoveryRequired(MachineRecoveryGuidance)` variant (the enum
  is not `#[non_exhaustive]`, `crates/keld-update/src/error.rs:21-42`) and its closed
  guidance enum with pinned texts; the mint-then-journal seam, which changes
  `WindowsExtractionRoot::begin_activation` and the signatures of
  `WindowsRecoveryInspection::recover` and `resume_unlaunched` so that minting and the
  first name-revealing record are separate calls, with its `WindowsMintedAttempt` and
  `WindowsJournaledAttempt` handles and its `AttemptOwner` and `InitiatingLogon`
  inputs (named for the KEL-270 T4d S3 public-API review), and whose coordinator input
  is the owner's `VerifiedWindowsImage` rather than a raw digest (S6b2; KEL-270
  owner decision `740998f4`, item 3); Machine-UAC admission in
  `load_windows_activation_write_snapshot` and `load_windows_recovery_inspection`,
  which today admit only `PerUserDirect` (`windows_baseline/load.rs:447-452`,
  `:657-672`, `:476-479`), through the existing
  `require_windows_machine_uac_owner_token` predicate rather than a second one; the
  recovery-role entry point for the D1 (refined) abandon intent, with the
  abandon-intent step mappings and `retirement_due` change behind it (§5); the
  executable-located entry point's closed
  image choice that replaces the fixed `HOST` constant (`windows_baseline/locate.rs:22`);
  the helper role entry points; the helper's self-anchor and helper-image derivation
  entry points and the signer rule that S9a moves into `keld-update`; the `keld-guard`
  Authenticode owner moved
  under D4, and its running-image entry with its typed refusal (S9d; KEL-270 owner
  decision `740998f4`, item 1); the `keld-ipc` `attempt` module with its endpoint
  locator; the `keld-update`
  health-receipt digest that the candidate computes for `AB1` (approved: KEL-270 owner
  decision `eff8e2fb`, 2026-10-06), exposed only through the attempt and candidate-boot
  accessors; S6's other surfaces, which are the claimant provenance and candidate-boot
  reads, the two-step `accept_health`, the launch liveness check and candidate launch,
  the `keld-ipc` attempt exchange with the breaking narrowing of the S4b codec items,
  the coordinator entry and the `keld-host` rendezvous argument (§6 S6a to S6c;
  Coordination record, Linear KEL-270 comment `7905ec8a-2529-4c23-90f4-878d315bc0e5`,
  2026-10-07); and the new safe
  wrappers that `keld-runtime`, `keld-guard` and `keld-ipc` export to the helper crate,
  with the `--recovery-role` selector constant and the breaking rename of
  `keld-runtime`'s `WindowsLpacChild` to `WindowsSuspendedChild` (S5); and S6b3's
  breaking return type of `install_host_death_job`, now the opaque `WindowsHostDeathJob`
  capability, with the new `release_for_exit`, `WindowsProcessJob::release_family`,
  `WindowsReleasedAttempt` and `WindowsExitCensus` ("Candidate release after commit";
  KEL-270 owner decision `740998f4`, item 2);
- permission model: yes — the install-mode protection profiles, UAC elevation and
  hostile-role denial decide who can mutate executable state, though no app grant is
  added. T4d adds the elevated helper principal, its recovery-only role and the
  initiating-user-only connect-back DACL, and S9d admits only the running image to the
  helper's self-anchor and the installed boot (KEL-270 owner decision `740998f4`,
  item 1). S6b3 adds none: the clear removes only a termination that the host could
  already perform on its own family, and it changes no principal, grant or DACL;
- dependency addition: none in Slice A; yes for T4d — the new workspace member
  `crates/keld-updater-helper` with internal edges only to `keld-update`, `keld-ipc`,
  `keld-runtime` and `keld-guard`; the `windows-sys` features `Win32_UI_Shell` and
  `Win32_System_Com` (`keld-runtime`) and `Win32_Security_Authentication_Identity`
  (`keld-guard`); and a Windows-only `keld-ipc` edge to the workspace-pinned `blake3`
  (`=1.8.7`) for the attempt-endpoint locator (approved: KEL-270 owner decision
  `eff8e2fb`, 2026-10-06), a crate that `keld-update` and
  `keld-pack` already lock. `Win32_Security_Authorization`, which the descriptor code
  uses, and the WinTrust and Cryptography features that the D4 move carries into
  `keld-guard` are already in the workspace `windows-sys` pin and are not new features.
  S9d adds to that pin any feature that its selected source or comparison needs, such
  as `Win32_System_ProcessStatus` for `GetMappedFileNameW` or `K32GetMappedFileNameW`
  (§5). S6b3 adds none: every call and constant of the clear and the census is under
  a `windows-sys` feature that `keld-runtime` already enables. No third-party crate is
  added to the workspace;
- wire protocol: yes — v0 bytes stay unchanged, but Slice-A delta-selection semantics
  and canonical package content are narrowed and require exact independent review; any
  new host/coordinator authentication channel remains separately owned and gated. T4d is
  wire-gated: the journal schema revision `keld.activation-journal/v2`
  (`initiating_logon`, `attempt_owner`, with the canonical encoding in §4); the
  `keld-attempt-<64 hex>` subprotocol namespace with its locator function, record
  layouts (approved: KEL-270 owner decision `eff8e2fb`, 2026-10-06)
  and closed message set (`BH1`,
  `BQ1`, `BA1`, `BR1`, `BO1`, `AH1`, `AC1`, `AA1`, `AR1`, `AB1`, `AY1`, `AF1`, `AK1`,
  owned by "Candidate connect-back" and pointed to from Architecture 02); the helper's
  fixed `--recovery-role` argument, a contract between a host and a helper of another
  version tree (S9b); and the
  canonical package content, which now requires `keld-updater-helper.exe`. S6b3 adds
  none.

## 9. Perf impact

No improvement claim. Slice A records download bytes, staging footprint, activation and
health latency. The 30-second window reuses KEL-70's default crash-window duration but
is stricter: any unexpected generation exit fails health. It bounds commit latency
without delaying candidate launch. A future delta must report CPU, memory, bytes,
fallback rate and end-to-end success before adding complexity.

## 10. Open questions

The product mode selection is approved; the following are implementation/evidence gates,
not requests to revisit that decision:

- T4b's common journal and persisted recovery have real Windows PerUserDirect
  crash-cut evidence; the attempt owner's attempt-bound health channel, 30-second `Ready`
  observation and installed-host lifecycle composition remain open.
- T4c must prove the default per-user install root, mode/provenance seeding and actual
  role write denial; the owning user's authority remains outside the threat claim.
- T4d must prove the Administrators/SYSTEM ACL, UAC cancellation with zero writes,
  over-the-shoulder user-token launch, and exact health/rollback under the live elevated
  owner. Reboot and owner death follow §4 "Machine-UAC recovery-required state and
  recovery-only role".
- Deferred T4d follow-up (owner: the KEL-53 owner, GYLDLAB, tracked under KEL-270 after
  slice S12): an optional simplification that finishes a `health-accepted` commit under
  the writer lease alone instead of the owner-loss proof. It is not part of T4d and
  needs its own reviewed amendment.
- S6d keeper under option B (owner: the KEL-53 owner, GYLDLAB; tracked under KEL-270
  before S6d starts): a keeper that the host starts inherits the host-death Job, is
  outside the attempt Job, and would be terminated by the S6b3 census at a committed
  exit; option B does not cover it ("Candidate release after commit"). S6d's own
  specification must solve this at the root before S6d starts; this amendment does not.
- Criterion-10 post-exit helper under option B (same owner): if such a helper is ever
  used, it inherits the host-death Job in the same way and needs the same root fix
  before it is specified.
- Candidate WebView2 profile during health under option B (same owner; tracked under
  KEL-270 before S6c starts): landed release boot selects its persistent user-data
  folder before any listener, child or window (Architecture 05 §1,
  `docs/architecture/05-webview-and-native.md:55-56`) and the KEL-135 lease excludes
  a second same-app host until the host owner exits (`05-webview-and-native.md:88`),
  so a candidate launched as landed would fail with `ProfileInUse` while the old host
  holds the lease and could never reach `Ready`. S6c must specify a profile handover,
  or a candidate-only profile, with no temporary-store fallback
  (`05-webview-and-native.md:64-65`), before it starts; this amendment does not (§6).
- T4e must close every host/attempt/authentication/replay/writer/lifecycle/health/recovery
  falsifier before any privileged seamless mechanism is selected. The task probe is only
  wake-up feasibility.
- KEL-254's installed-image consumer and KEL-96's host admission must consume the exact
  updater selection under each admitted direct-mode profile without acquiring updater
  write authority. KEL-135 remains sole publisher/app/profile identity owner.

Manifest/full verification, logical provenance admission, Windows packaging and protected
incomplete extraction have landed. T4a initialization and every activation/mode cell
retain their separate native acceptance; this specification does not claim them shipped.
